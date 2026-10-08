//! Données d'usage persistées : file d'attente et historique d'écoute.
//!
//! Distinct de [`crate::config`] (préférences) : ici, ce que l'application
//! accumule au fil des sessions. Stocké dans
//! `~/.local/share/waveline/state.json` (répertoire XDG de données), écrit de
//! façon **atomique** (fichier temporaire puis renommage) pour qu'un arrêt
//! brutal ne laisse jamais un JSON tronqué.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::Track;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// File d'attente, dans l'ordre de lecture.
    #[serde(default)]
    pub queue: Vec<Track>,
    /// Historique, le plus récent en tête.
    #[serde(default)]
    pub history: Vec<Track>,
}

impl State {
    fn path() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("waveline").join("state.json"))
    }

    /// Charge l'état, ou un état vide en cas d'absence/erreur.
    pub fn load() -> State {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    /// Analyse un état JSON ; les champs absents sont vides.
    pub fn parse(json: &str) -> Option<State> {
        serde_json::from_str(json).ok()
    }

    /// Écrit l'état sur disque (atomiquement). Les erreurs sont ignorées :
    /// perdre la persistance ne doit jamais interrompre la lecture.
    pub fn save(&self) {
        let Some(p) = Self::path() else {
            return;
        };
        if let Ok(json) = serde_json::to_string(self) {
            let _ = write_atomic(&p, json.as_bytes());
        }
    }
}

/// Écrit `bytes` dans `path` via un fichier temporaire voisin puis `rename`
/// (atomique sur un même système de fichiers). Crée les dossiers parents.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Platform;

    fn track(title: &str) -> Track {
        Track {
            platform: Platform::Mixcloud,
            id: format!("u/{title}"),
            title: title.into(),
            artist: "a".into(),
            permalink: format!("https://www.mixcloud.com/u/{title}/"),
            duration_ms: Some(1_000),
        }
    }

    #[test]
    fn round_trip_json() {
        let s = State {
            queue: vec![track("q1")],
            history: vec![track("h1"), track("h2")],
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(State::parse(&json).unwrap(), s);
    }

    #[test]
    fn champs_absents_ou_json_invalide() {
        let s = State::parse("{}").unwrap();
        assert!(s.queue.is_empty() && s.history.is_empty());
        assert_eq!(State::parse("not json"), None);
    }

    #[test]
    fn ecriture_atomique_cree_les_dossiers_et_ne_laisse_pas_de_temporaire() {
        let dir = std::env::temp_dir().join(format!(
            "waveline-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("nested").join("state.json");
        write_atomic(&path, b"{\"queue\":[]}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"queue\":[]}");
        assert!(!dir.join("nested").join("state.json.tmp").exists());
        // Réécriture : remplace le contenu.
        write_atomic(&path, b"{}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
