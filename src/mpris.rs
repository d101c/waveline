//! Intégration MPRIS (D-Bus) : expose `org.mpris.MediaPlayer2.waveline` pour
//! que les touches média du clavier (GNOME et autres bureaux), les contrôles
//! du panneau et l'écran de verrouillage pilotent la lecture.
//!
//! Tout vit sur un thread dédié ; les appels de méthode D-Bus sont convertis en
//! [`MediaCommand`] envoyées à la boucle principale (routées comme les touches
//! clavier). En l'absence de bus de session, le thread se termine en silence —
//! waveline fonctionne sans, simplement sans intégration média.

use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use mpris_server::zbus::{self, fdo};
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, Property, RootInterface,
    Server, Time, TrackId, Volume,
};

use crate::audio::Shared;

/// Commande issue d'un contrôle média externe (touche, panneau, client MPRIS).
#[derive(Debug, Clone, Copy)]
pub enum MediaCommand {
    PlayPause,
    Play,
    Pause,
    Next,
    Prev,
    Stop,
    SetVolume(u8),
    Quit,
}

/// Implémentation des interfaces MPRIS, adossée à l'état partagé du moteur.
struct Imp {
    shared: Arc<Shared>,
    tx: Mutex<Sender<MediaCommand>>,
}

impl Imp {
    fn send(&self, c: MediaCommand) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(c);
        }
    }

    fn status(&self) -> PlaybackStatus {
        if self.shared.playing.load(Ordering::Relaxed) {
            PlaybackStatus::Playing
        } else if self.shared.loading.load(Ordering::Relaxed)
            || self.shared.now.lock().map(|n| n.is_some()).unwrap_or(false)
        {
            PlaybackStatus::Paused
        } else {
            PlaybackStatus::Stopped
        }
    }

    fn current_id(&self) -> String {
        self.shared
            .now
            .lock()
            .ok()
            .and_then(|n| n.as_ref().map(|t| t.id.clone()))
            .unwrap_or_default()
    }

    fn build_metadata(&self) -> Metadata {
        let now = self.shared.now.lock().ok().and_then(|n| n.clone());
        let mut b = Metadata::builder();
        if let Some(t) = now {
            b = b.title(t.title.clone()).artist([t.artist.clone()]);
            if let Some(ms) = t.duration_ms {
                b = b.length(Time::from_millis(ms as i64));
            }
        }
        b.build()
    }
}

impl RootInterface for Imp {
    async fn identity(&self) -> fdo::Result<String> {
        Ok("waveline".into())
    }
    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("waveline".into())
    }
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }
    async fn quit(&self) -> fdo::Result<()> {
        self.send(MediaCommand::Quit);
        Ok(())
    }
    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn set_fullscreen(&self, _: bool) -> zbus::Result<()> {
        Ok(())
    }
    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec!["https".into()])
    }
    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }
}

impl PlayerInterface for Imp {
    async fn next(&self) -> fdo::Result<()> {
        self.send(MediaCommand::Next);
        Ok(())
    }
    async fn previous(&self) -> fdo::Result<()> {
        self.send(MediaCommand::Prev);
        Ok(())
    }
    async fn pause(&self) -> fdo::Result<()> {
        self.send(MediaCommand::Pause);
        Ok(())
    }
    async fn play_pause(&self) -> fdo::Result<()> {
        self.send(MediaCommand::PlayPause);
        Ok(())
    }
    async fn stop(&self) -> fdo::Result<()> {
        self.send(MediaCommand::Stop);
        Ok(())
    }
    async fn play(&self) -> fdo::Result<()> {
        self.send(MediaCommand::Play);
        Ok(())
    }
    async fn seek(&self, _offset: Time) -> fdo::Result<()> {
        Ok(())
    }
    async fn set_position(&self, _track: TrackId, _pos: Time) -> fdo::Result<()> {
        Ok(())
    }
    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(self.status())
    }
    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }
    async fn set_loop_status(&self, _: LoopStatus) -> zbus::Result<()> {
        Ok(())
    }
    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn set_rate(&self, _: PlaybackRate) -> zbus::Result<()> {
        Ok(())
    }
    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn set_shuffle(&self, _: bool) -> zbus::Result<()> {
        Ok(())
    }
    async fn metadata(&self) -> fdo::Result<Metadata> {
        Ok(self.build_metadata())
    }
    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.shared.volume.load(Ordering::Relaxed) as f64 / 100.0)
    }
    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        let pct = (volume.clamp(0.0, 1.0) * 100.0).round() as u8;
        self.send(MediaCommand::SetVolume(pct));
        Ok(())
    }
    async fn position(&self) -> fdo::Result<Time> {
        Ok(Time::from_millis(
            self.shared.position_ms.load(Ordering::Relaxed) as i64,
        ))
    }
    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

/// Démarre le serveur MPRIS sur un thread dédié. No-op silencieux sans bus.
pub fn start(shared: Arc<Shared>, tx: Sender<MediaCommand>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("waveline-mpris".into())
        .spawn(move || {
            futures_lite::future::block_on(async move {
                let imp = Imp {
                    shared,
                    tx: Mutex::new(tx),
                };
                let server = match Server::new("waveline", imp).await {
                    Ok(s) => s,
                    Err(_) => return, // pas de bus de session : on abandonne en silence
                };

                // Publie les changements d'état pour l'affichage (verrou, panneau).
                let mut last_status: Option<PlaybackStatus> = None;
                let mut last_id = String::new();
                loop {
                    async_io::Timer::after(Duration::from_millis(700)).await;
                    let status = server.imp().status();
                    let id = server.imp().current_id();
                    let mut props = Vec::new();
                    if Some(status) != last_status {
                        last_status = Some(status);
                        props.push(Property::PlaybackStatus(status));
                    }
                    if id != last_id {
                        last_id = id;
                        props.push(Property::Metadata(server.imp().build_metadata()));
                    }
                    if !props.is_empty() {
                        let _ = server.properties_changed(props).await;
                    }
                }
            });
        })
        .expect("thread mpris")
}
