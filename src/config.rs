//! Configuration persistante : les pseudos de compte saisis par l'utilisateur.
//!
//! Stockée en clair dans `~/.config/waveline/config.json` — ce ne sont que des
//! noms d'utilisateur publics (pas de secret, pas de token).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Pseudo SoundCloud (la partie après soundcloud.com/).
    pub soundcloud: Option<String>,
    /// Pseudo Mixcloud.
    pub mixcloud: Option<String>,
}

impl Config {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("waveline").join("config.json"))
    }

    /// Charge la config, ou renvoie une config vide en cas d'absence/erreur.
    pub fn load() -> Config {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Écrit la config sur disque (création du dossier si besoin).
    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}

/// Normalise une saisie de pseudo : vide → `None`, sinon nettoie l'URL/espaces.
pub fn normalize_handle(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // Accepte une URL complète et en extrait le pseudo.
    let s = s
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_start_matches("soundcloud.com/")
        .trim_start_matches("mixcloud.com/")
        .trim_matches('/');
    let handle = s.split('/').next().unwrap_or(s).trim();
    if handle.is_empty() {
        None
    } else {
        Some(handle.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_pseudos_et_urls() {
        assert_eq!(normalize_handle("  bonobo "), Some("bonobo".into()));
        assert_eq!(
            normalize_handle("https://soundcloud.com/flume"),
            Some("flume".into())
        );
        assert_eq!(
            normalize_handle("www.mixcloud.com/NTSRadio/"),
            Some("NTSRadio".into())
        );
        assert_eq!(normalize_handle("   "), None);
    }
}
