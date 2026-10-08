//! Modèles de données unifiés pour les deux plateformes.
//!
//! L'idée centrale de waveline : SoundCloud et Mixcloud exposent des objets
//! différents (track vs cloudcast), mais l'UI ne manipule qu'un seul type
//! unifié [`Track`]. Chaque provider traduit ses objets vers ce modèle.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Plateforme d'origine d'un morceau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    SoundCloud,
    Mixcloud,
}

impl Platform {
    /// Étiquette courte affichée dans les listes (colonne plateforme).
    pub fn tag(self) -> &'static str {
        match self {
            Platform::SoundCloud => "SC",
            Platform::Mixcloud => "MC",
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Platform::SoundCloud => "SoundCloud",
            Platform::Mixcloud => "Mixcloud",
        })
    }
}

/// Un morceau / mix unifié, indépendant de la plateforme.
///
/// Sérialisable pour la persistance de la file et de l'historique
/// (`~/.local/share/waveline/state.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub platform: Platform,
    /// Identifiant stable côté plateforme (urn SoundCloud, key Mixcloud).
    pub id: String,
    pub title: String,
    pub artist: String,
    /// URL canonique de la page (permet la résolution du flux a posteriori).
    pub permalink: String,
    /// Durée en millisecondes, si connue.
    pub duration_ms: Option<u64>,
}

impl Track {
    /// Deux entrées désignent le même morceau si plateforme et identifiant
    /// coïncident (le titre ou la durée peuvent varier entre deux réponses API).
    /// À défaut d'identifiant commun (morceau de démonstration ou URL collée,
    /// dont l'`id` n'est pas celui de l'API), un même permalink suffit.
    pub fn same_as(&self, other: &Track) -> bool {
        if self.platform != other.platform {
            return false;
        }
        if self.id == other.id {
            return true;
        }
        let a = self.permalink.trim_end_matches('/');
        let b = other.permalink.trim_end_matches('/');
        !a.is_empty() && a.eq_ignore_ascii_case(b)
    }

    /// Durée formatée `H:MM:SS` ou `M:SS`, ou `--:--` si inconnue.
    pub fn duration_human(&self) -> String {
        match self.duration_ms {
            Some(ms) => fmt_duration(ms),
            None => "--:--".to_string(),
        }
    }
}

/// Formate une durée en millisecondes : `1:02:11` ou `4:50`.
pub fn fmt_duration(ms: u64) -> String {
    let total = ms / 1000;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formate_les_durees_courtes_et_longues() {
        assert_eq!(fmt_duration(290_000), "4:50");
        assert_eq!(fmt_duration(3_731_000), "1:02:11");
        assert_eq!(fmt_duration(0), "0:00");
    }

    #[test]
    fn same_as_compare_l_id_puis_le_permalink() {
        let mk = |p, id: &str, link: &str| Track {
            platform: p,
            id: id.into(),
            title: "t".into(),
            artist: "a".into(),
            permalink: link.into(),
            duration_ms: None,
        };
        let api = mk(
            Platform::SoundCloud,
            "soundcloud:tracks:1",
            "https://soundcloud.com/a/t",
        );
        let demo = mk(
            Platform::SoundCloud,
            "https://soundcloud.com/a/t",
            "https://soundcloud.com/a/t/",
        );
        assert!(api.same_as(&demo), "même permalink (slash final ignoré)");
        assert!(api.same_as(&api));
        let other = mk(
            Platform::SoundCloud,
            "soundcloud:tracks:2",
            "https://soundcloud.com/a/u",
        );
        assert!(!api.same_as(&other));
        let mc = mk(
            Platform::Mixcloud,
            "soundcloud:tracks:1",
            "https://soundcloud.com/a/t",
        );
        assert!(!api.same_as(&mc), "plateformes différentes");
        let empty_a = mk(Platform::SoundCloud, "x", "");
        let empty_b = mk(Platform::SoundCloud, "y", "");
        assert!(
            !empty_a.same_as(&empty_b),
            "permalinks vides : jamais égaux"
        );
    }

    #[test]
    fn duree_inconnue_affiche_placeholder() {
        let t = Track {
            platform: Platform::SoundCloud,
            id: "x".into(),
            title: "t".into(),
            artist: "a".into(),
            permalink: "https://soundcloud.com/a/t".into(),
            duration_ms: None,
        };
        assert_eq!(t.duration_human(), "--:--");
    }
}
