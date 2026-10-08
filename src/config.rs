//! Configuration persistante : préférences de l'utilisateur (pseudos de compte,
//! langue, volume, style de visualiseur).
//!
//! Stockée en clair dans `~/.config/waveline/config.json` — ce ne sont que des
//! préférences et des noms d'utilisateur publics (pas de secret, pas de token).
//! Les données d'usage (file d'attente, historique) vivent à part, dans
//! [`crate::state`], pour séparer « ce que l'utilisateur règle » de « ce que
//! l'application accumule ».

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::VizMode;
use crate::i18n::Lang;

/// Volume par défaut (%), utilisé pour une config absente ou ancienne.
pub const DEFAULT_VOLUME: u8 = 80;

fn default_volume() -> u8 {
    DEFAULT_VOLUME
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Pseudo SoundCloud (la partie après soundcloud.com/).
    pub soundcloud: Option<String>,
    /// Pseudo Mixcloud.
    pub mixcloud: Option<String>,
    /// Langue de l'interface (anglais par défaut, absent des anciennes configs).
    #[serde(default)]
    pub lang: Lang,
    /// Volume (0..=100), mémorisé d'une session à l'autre.
    #[serde(default = "default_volume")]
    pub volume: u8,
    /// Style de visualiseur courant.
    #[serde(default)]
    pub viz: VizMode,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            soundcloud: None,
            mixcloud: None,
            lang: Lang::default(),
            volume: DEFAULT_VOLUME,
            viz: VizMode::default(),
        }
    }
}

impl Config {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("waveline").join("config.json"))
    }

    /// Charge la config, ou renvoie une config vide en cas d'absence/erreur.
    pub fn load() -> Config {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    /// Analyse une config JSON ; les champs absents prennent leur valeur par
    /// défaut (compatibilité ascendante) ; le volume est borné à 100.
    fn parse(json: &str) -> Option<Config> {
        let mut c: Config = serde_json::from_str(json).ok()?;
        c.volume = c.volume.min(100);
        Some(c)
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
    fn ancienne_config_sans_volume_ni_viz_prend_les_defauts() {
        let c = Config::parse(r#"{"soundcloud":"bonobo","mixcloud":null}"#).unwrap();
        assert_eq!(c.soundcloud.as_deref(), Some("bonobo"));
        assert_eq!(c.lang, Lang::En);
        assert_eq!(c.volume, DEFAULT_VOLUME);
        assert_eq!(c.viz, VizMode::Bars);
    }

    #[test]
    fn config_complete_round_trip_et_volume_borne() {
        let c = Config {
            soundcloud: None,
            mixcloud: Some("NTSRadio".into()),
            lang: Lang::Fr,
            volume: 55,
            viz: VizMode::Scope,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back = Config::parse(&json).unwrap();
        assert_eq!(back.volume, 55);
        assert_eq!(back.viz, VizMode::Scope);
        assert_eq!(back.lang, Lang::Fr);
        // Un volume hors bornes (fichier édité à la main) est ramené à 100.
        let c = Config::parse(r#"{"volume":250}"#).unwrap();
        assert_eq!(c.volume, 100);
    }

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
