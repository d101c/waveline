//! Configuration persistante : préférences de l'utilisateur (pseudos de compte,
//! langue, volume, style de visualiseur).
//!
//! Stockée en clair dans `~/.config/waveline/config.json` — ce ne sont que des
//! préférences et des noms d'utilisateur publics (pas de secret, pas de token).
//! Les données d'usage (file d'attente, historique) vivent à part, dans
//! [`crate::state`], pour séparer « ce que l'utilisateur règle » de « ce que
//! l'application accumule ».
//!
//! Le fichier peut être édité à la main : la lecture est **tolérante champ par
//! champ** (une valeur invalide reprend sa valeur par défaut sans effacer les
//! autres), un fichier qui n'est plus du JSON est mis de côté en `.bak` plutôt
//! qu'écrasé, et l'écriture est atomique (cf. [`crate::state::write_atomic`]).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::VizMode;
use crate::i18n::Lang;

/// Volume par défaut (%), utilisé pour une config absente ou ancienne.
pub const DEFAULT_VOLUME: u8 = 80;

fn default_volume() -> u8 {
    DEFAULT_VOLUME
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Charge la config, ou renvoie une config vide en cas d'absence.
    ///
    /// Un fichier présent mais illisible (JSON tronqué, virgule finale…) est
    /// renommé en `config.json.bak` : les pseudos qu'il contenait restent
    /// récupérables au lieu d'être écrasés par les défauts à la sortie.
    pub fn load() -> Config {
        let Some(p) = Self::path() else {
            return Config::default();
        };
        let Ok(bytes) = std::fs::read(&p) else {
            return Config::default();
        };
        let parsed = std::str::from_utf8(&bytes).ok().and_then(Self::parse);
        match parsed {
            Some(c) => c,
            None => {
                if !bytes.iter().all(u8::is_ascii_whitespace) {
                    let _ = std::fs::rename(&p, p.with_extension("json.bak"));
                }
                Config::default()
            }
        }
    }

    /// Analyse une config JSON champ par champ : un champ absent ou invalide
    /// (type faux, valeur hors bornes, variante inconnue) reprend sa valeur par
    /// défaut sans invalider les autres. Renvoie `None` seulement si le texte
    /// n'est pas un objet JSON.
    fn parse(json: &str) -> Option<Config> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let obj = v.as_object()?;
        let d = Config::default();
        let field = |k: &str| obj.get(k).cloned();
        let handle = |k: &str| -> Option<String> {
            field(k)
                .and_then(|x| x.as_str().map(str::to_owned))
                .and_then(|s| normalize_handle(&s))
        };
        Some(Config {
            soundcloud: handle("soundcloud"),
            mixcloud: handle("mixcloud"),
            lang: field("lang")
                .and_then(|x| serde_json::from_value(x).ok())
                .unwrap_or(d.lang),
            volume: field("volume")
                .and_then(|x| x.as_f64())
                .map(|f| f.clamp(0.0, 100.0) as u8)
                .unwrap_or(d.volume),
            viz: field("viz")
                .and_then(|x| serde_json::from_value(x).ok())
                .unwrap_or(d.viz),
        })
    }

    /// Écrit la config sur disque, atomiquement (dossier créé si besoin).
    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = crate::state::write_atomic(&p, s.as_bytes());
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
    fn une_valeur_invalide_n_efface_pas_les_autres_champs() {
        // Volume hors u8, langue inconnue, viz en minuscules : chaque champ fautif
        // reprend son défaut, les pseudos sont conservés.
        let c = Config::parse(
            r#"{"soundcloud":"bonobo","mixcloud":"https://www.mixcloud.com/NTSRadio/","volume":300,"lang":"fr","viz":"bars"}"#,
        )
        .unwrap();
        assert_eq!(c.soundcloud.as_deref(), Some("bonobo"));
        assert_eq!(c.mixcloud.as_deref(), Some("NTSRadio"), "pseudo normalisé");
        assert_eq!(c.volume, 100, "borné");
        assert_eq!(c.lang, Lang::En);
        assert_eq!(c.viz, VizMode::Bars);
        let c = Config::parse(r#"{"soundcloud":"x","volume":-5}"#).unwrap();
        assert_eq!(c.volume, 0);
        let c = Config::parse(r#"{"soundcloud":"x","volume":"80"}"#).unwrap();
        assert_eq!(c.volume, DEFAULT_VOLUME);
        let c = Config::parse(r#"{"soundcloud":42,"volume":33.7}"#).unwrap();
        assert_eq!(c.soundcloud, None);
        assert_eq!(c.volume, 33);
        // Pas un objet JSON : refusé (le fichier sera mis de côté).
        assert_eq!(Config::parse("{\"soundcloud\":\"x\",}"), None);
        assert_eq!(Config::parse("[]"), None);
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
