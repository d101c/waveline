//! Sortie audio : on écrit du PCM brut (S16LE entrelacé) sur l'entrée standard
//! d'un lecteur système déjà présent (`pw-play` PipeWire, ou `aplay` ALSA).
//!
//! Ce choix évite de lier `libasound`/`cpal` à la compilation (pas de headers
//! dev requis) tout en restant « sans dépendance Rust supplémentaire ». Un
//! backend `cpal` natif pourra être ajouté derrière un feature flag.

use std::io::{self, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

/// Un puits audio actif : un processus lecteur alimenté en PCM via stdin.
pub struct Sink {
    child: Child,
    stdin: ChildStdin,
    buf: Vec<u8>,
}

impl Sink {
    /// Ouvre un lecteur pour le format donné. Essaie PipeWire puis ALSA.
    pub fn open(sample_rate: u32, channels: u16) -> io::Result<Sink> {
        let mut last_err = None;
        for backend in Backend::ALL {
            match backend.spawn(sample_rate, channels) {
                Ok(mut child) => {
                    let stdin = child.stdin.take().expect("stdin demandé");
                    return Ok(Sink {
                        child,
                        stdin,
                        buf: Vec::with_capacity(8192),
                    });
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "aucun lecteur PCM (pw-play/aplay)")
        }))
    }

    /// Écrit des échantillons i16 entrelacés, atténués par `volume` (0..=100).
    pub fn write(&mut self, samples: &[i16], volume: u8) -> io::Result<()> {
        self.buf.clear();
        self.buf.reserve(samples.len() * 2);
        let v = volume.min(100) as i32;
        for &s in samples {
            let scaled = if v == 100 {
                s
            } else {
                ((s as i32 * v) / 100) as i16
            };
            self.buf.extend_from_slice(&scaled.to_le_bytes());
        }
        self.stdin.write_all(&self.buf)
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        // Coupe l'entrée puis tue le lecteur pour ne pas laisser de son résiduel.
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Les backends de sortie supportés, par ordre de préférence.
#[derive(Clone, Copy)]
enum Backend {
    PipeWire,
    Alsa,
}

impl Backend {
    const ALL: [Backend; 2] = [Backend::PipeWire, Backend::Alsa];

    fn spawn(self, sr: u32, ch: u16) -> io::Result<Child> {
        let mut cmd = match self {
            Backend::PipeWire => {
                let mut c = Command::new("pw-play");
                c.args([
                    "--raw",
                    "--format=s16",
                    &format!("--rate={sr}"),
                    &format!("--channels={ch}"),
                    "-",
                ]);
                c
            }
            Backend::Alsa => {
                let mut c = Command::new("aplay");
                c.args([
                    "-q",
                    "-t",
                    "raw",
                    "-f",
                    "S16_LE",
                    "-r",
                    &sr.to_string(),
                    "-c",
                    &ch.to_string(),
                    "-",
                ]);
                c
            }
        };
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
}
