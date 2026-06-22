//! Modèles de données unifiés pour les deux plateformes.
//!
//! L'idée centrale de waveline : SoundCloud et Mixcloud exposent des objets
//! différents (track vs cloudcast), mais l'UI ne manipule qu'un seul type
//! unifié [`Track`]. Chaque provider traduit ses objets vers ce modèle.

use std::fmt;

/// Plateforme d'origine d'un morceau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone)]
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
