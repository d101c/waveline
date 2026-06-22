//! Contrôleur de lecture audio.
//!
//! Un thread worker reçoit des [`Command`] et, pour chaque morceau, résout le
//! flux (réseau), le décode avec symphonia et l'envoie au [`Sink`]. L'UI
//! n'interagit qu'avec [`Player`] : elle envoie des commandes et lit un état
//! partagé (position, durée, lecture/pause, morceau courant, erreur) sans
//! jamais se bloquer.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::spectrum::{self, BANDS};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::sink::Sink;
use super::source::{HlsReader, HttpStream};
use crate::model::Track;
use crate::providers::{self, Container, StreamKind};

/// Commandes envoyées au worker de lecture.
enum Command {
    Play(String), // URL/permalink à résoudre puis jouer
    Pause,
    Resume,
    Stop,
    Quit,
}

/// État partagé entre le worker et l'UI (lecture sans verrou pour les scalaires).
pub struct Shared {
    pub position_ms: AtomicU64,
    pub duration_ms: AtomicU64,
    pub playing: AtomicBool,
    pub loading: AtomicBool,
    pub volume: AtomicU8,
    /// Incrémenté quand un morceau se termine naturellement (l'UI enchaîne).
    pub finished_generation: AtomicU64,
    pub now: Mutex<Option<Track>>,
    pub error: Mutex<Option<String>>,
    /// Amplitudes du spectre par bande (0..1), pour l'analyseur visuel.
    pub spectrum: Mutex<[f32; BANDS]>,
    /// Échantillons mono récents (~[-1,1]) pour le mode oscilloscope.
    pub waveform: Mutex<Vec<f32>>,
}

/// Nombre de points de forme d'onde publiés (rééchantillonnés à l'affichage).
pub const WAVE_POINTS: usize = 256;

impl Shared {
    fn new(volume: u8) -> Arc<Shared> {
        Arc::new(Shared {
            position_ms: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            loading: AtomicBool::new(false),
            volume: AtomicU8::new(volume),
            finished_generation: AtomicU64::new(0),
            now: Mutex::new(None),
            error: Mutex::new(None),
            spectrum: Mutex::new([0.0; BANDS]),
            waveform: Mutex::new(Vec::new()),
        })
    }

    /// Remet le spectre et la forme d'onde à zéro (pause, stop, fin).
    fn clear_spectrum(&self) {
        *self.spectrum.lock().unwrap() = [0.0; BANDS];
        self.waveform.lock().unwrap().clear();
    }
}

/// Façade côté UI.
pub struct Player {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl Player {
    pub fn new(volume: u8) -> Player {
        let (tx, rx) = mpsc::channel();
        let shared = Shared::new(volume);
        let worker_shared = shared.clone();
        let handle = thread::Builder::new()
            .name("waveline-audio".into())
            .spawn(move || worker(rx, worker_shared))
            .expect("thread audio");
        Player {
            tx,
            shared,
            handle: Some(handle),
        }
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// Lance la lecture d'une URL (résolution faite dans le worker).
    pub fn play_url(&self, url: impl Into<String>) {
        self.shared.loading.store(true, Ordering::Relaxed);
        *self.shared.error.lock().unwrap() = None;
        let _ = self.tx.send(Command::Play(url.into()));
    }

    pub fn pause(&self) {
        let _ = self.tx.send(Command::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Command::Resume);
    }

    /// Bascule lecture/pause selon l'état courant.
    pub fn toggle(&self) {
        if self.shared.playing.load(Ordering::Relaxed) {
            self.pause();
        } else {
            self.resume();
        }
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
    }

    pub fn set_volume(&self, v: u8) {
        self.shared.volume.store(v.min(100), Ordering::Relaxed);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Quit);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Boucle principale du worker : attend un Play, joue, recommence.
fn worker(rx: Receiver<Command>, shared: Arc<Shared>) {
    let agent = crate::http::agent();
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::Play(url) => {
                match play_one(&agent, &rx, &shared, &url) {
                    Ok(true) => break, // un Quit est remonté pendant la lecture
                    Ok(false) => {}
                    Err(e) => set_error(&shared, e),
                }
                shared.playing.store(false, Ordering::Relaxed);
                shared.loading.store(false, Ordering::Relaxed);
            }
            Command::Quit => break,
            // Hors lecture : pause/resume/stop sont sans effet.
            _ => {}
        }
    }
}

/// Issue de la boucle de décodage d'un morceau.
enum Flow {
    Finished,
    Stopped,
    Switch(String),
    Quit,
}

/// Joue un morceau. Retourne `Ok(true)` si le worker doit s'arrêter (Quit).
fn play_one(
    agent: &ureq::Agent,
    rx: &Receiver<Command>,
    shared: &Arc<Shared>,
    url: &str,
) -> Result<bool, String> {
    // 1. Résolution réseau (peut échouer proprement).
    let (track, source) =
        providers::resolve_url(agent, url).map_err(|e| e.to_string())?;
    let duration = track.duration_ms.unwrap_or(0);
    shared.duration_ms.store(duration, Ordering::Relaxed);
    shared.position_ms.store(0, Ordering::Relaxed);
    *shared.now.lock().unwrap() = Some(track);
    shared.loading.store(false, Ordering::Relaxed);

    // 2. Construit la source d'octets selon le type de flux.
    let media: Box<dyn symphonia::core::io::MediaSource> = match source.kind {
        StreamKind::Progressive(u) => {
            Box::new(HttpStream::open(agent, &u).map_err(|e| e.to_string())?)
        }
        StreamKind::HlsSegments(segs) => Box::new(HlsReader::new(agent.clone(), segs)),
    };

    // 3. Décode et joue.
    match decode_loop(rx, shared, media, source.container)? {
        Flow::Finished => {
            shared.finished_generation.fetch_add(1, Ordering::Relaxed);
            Ok(false)
        }
        Flow::Stopped => Ok(false),
        Flow::Switch(next) => {
            // Enchaîne immédiatement sur le nouveau morceau demandé.
            shared.loading.store(true, Ordering::Relaxed);
            play_one(agent, rx, shared, &next)
        }
        Flow::Quit => Ok(true),
    }
}

fn decode_loop(
    rx: &Receiver<Command>,
    shared: &Arc<Shared>,
    media: Box<dyn symphonia::core::io::MediaSource>,
    container: Container,
) -> Result<Flow, String> {
    let mss = MediaSourceStream::new(media, Default::default());
    let mut hint = Hint::new();
    match container {
        Container::Mp3 => hint.with_extension("mp3"),
        Container::Mp4 => hint.with_extension("m4a"),
        Container::Ogg => hint.with_extension("ogg"),
        Container::Unknown => &mut hint,
    };

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("format audio non reconnu : {e}"))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| "aucune piste audio décodable".to_string())?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("codec non supporté : {e}"))?;

    let mut sink: Option<Sink> = None;
    let mut sample_buf: Option<SampleBuffer<i16>> = None;
    let mut spec_rate = 0u32;
    let mut frames_total: u64 = 0;
    let mut paused = false;
    // Buffer mono glissant + horodatage pour throttler l'analyseur de spectre.
    let mut mono: Vec<f32> = Vec::with_capacity(spectrum::FFT_SIZE * 2);
    let mut last_fft = Instant::now();

    shared.playing.store(true, Ordering::Relaxed);

    loop {
        // --- Traite les commandes en attente (non bloquant en lecture) ---
        loop {
            let cmd = if paused { rx.recv().ok() } else { rx.try_recv().ok() };
            match cmd {
                Some(Command::Pause) => {
                    paused = true;
                    sink = None; // coupe le son immédiatement
                    shared.playing.store(false, Ordering::Relaxed);
                    shared.clear_spectrum();
                }
                Some(Command::Resume) => {
                    paused = false;
                    shared.playing.store(true, Ordering::Relaxed);
                }
                Some(Command::Stop) => {
                    shared.position_ms.store(0, Ordering::Relaxed);
                    shared.duration_ms.store(0, Ordering::Relaxed);
                    *shared.now.lock().unwrap() = None;
                    shared.clear_spectrum();
                    return Ok(Flow::Stopped);
                }
                Some(Command::Play(u)) => return Ok(Flow::Switch(u)),
                Some(Command::Quit) => return Ok(Flow::Quit),
                None => {
                    if paused {
                        // recv a renvoyé None => canal fermé.
                        return Ok(Flow::Quit);
                    }
                    break;
                }
            }
            if !paused {
                break;
            }
        }

        // --- Décode le paquet suivant ---
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                return Ok(Flow::Finished);
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                return Ok(Flow::Finished);
            }
            Err(_) => return Ok(Flow::Finished),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue, // paquet abîmé, on saute
            Err(_) => return Ok(Flow::Finished),
        };

        let spec = *decoded.spec();
        let frames = decoded.frames();
        if sample_buf.is_none() {
            spec_rate = spec.rate;
            sample_buf = Some(SampleBuffer::<i16>::new(decoded.capacity() as u64, spec));
        }
        let sbuf = sample_buf.as_mut().unwrap();
        sbuf.copy_interleaved_ref(decoded);

        // (Ré)ouvre le sink si besoin (premier paquet ou après une pause).
        if sink.is_none() {
            match Sink::open(spec.rate, spec.channels.count() as u16) {
                Ok(s) => sink = Some(s),
                Err(e) => return Err(format!("sortie audio indisponible : {e}")),
            }
        }
        let vol = shared.volume.load(Ordering::Relaxed);
        if let Some(s) = sink.as_mut() {
            if let Err(e) = s.write(sbuf.samples(), vol) {
                return Err(format!("écriture audio : {e}"));
            }
        }

        frames_total += frames as u64;
        if spec_rate > 0 {
            let pos = frames_total * 1000 / spec_rate as u64;
            shared.position_ms.store(pos, Ordering::Relaxed);
        }

        // --- Visualiseurs (throttlé ~30 Hz, coût négligeable) ---
        push_mono(&mut mono, sbuf.samples(), spec.channels.count());
        if last_fft.elapsed() >= Duration::from_millis(33) && mono.len() >= spectrum::FFT_SIZE {
            let window = &mono[mono.len() - spectrum::FFT_SIZE..];
            // Spectre : attaque immédiate, décroissance vive (réactif).
            let raw = spectrum::compute_bands(window);
            if let Ok(mut s) = shared.spectrum.lock() {
                for i in 0..BANDS {
                    s[i] = raw[i].max(s[i] * 0.72);
                }
            }
            // Forme d'onde : sous-échantillonne la fenêtre en WAVE_POINTS points.
            if let Ok(mut w) = shared.waveform.lock() {
                w.clear();
                let step = (window.len() / WAVE_POINTS).max(1);
                w.extend(window.iter().step_by(step).take(WAVE_POINTS).copied());
            }
            if mono.len() > spectrum::FFT_SIZE * 2 {
                let cut = mono.len() - spectrum::FFT_SIZE;
                mono.drain(..cut);
            }
            last_fft = Instant::now();
        }
    }
}

/// Ajoute au buffer mono le mixage des échantillons i16 entrelacés (→ f32).
fn push_mono(buf: &mut Vec<f32>, samples: &[i16], channels: usize) {
    if channels == 0 {
        return;
    }
    for frame in samples.chunks(channels) {
        let sum: i32 = frame.iter().map(|&s| s as i32).sum();
        buf.push(sum as f32 / (channels as f32 * 32768.0));
    }
}

fn set_error(shared: &Arc<Shared>, msg: String) {
    if !msg.is_empty() {
        *shared.error.lock().unwrap() = Some(msg);
    }
}
