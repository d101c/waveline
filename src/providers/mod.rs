//! Couche providers : traduit une URL/recherche SoundCloud ou Mixcloud vers le
//! modèle unifié [`Track`](crate::model::Track) et résout un flux jouable.
//!
//! Le reste de l'app ne dépend que de ces types ; ajouter une 3ᵉ plateforme =
//! une nouvelle implémentation derrière la même interface.

pub mod hls;
pub mod mixcloud;
pub mod soundcloud;

use crate::http::HttpError;
use crate::model::{Platform, Track};

/// Conteneur/codec d'un flux, pour aiguiller le décodeur audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mp3,
    Mp4,
    Ogg,
    Unknown,
}

impl Container {
    /// Devine le conteneur depuis un type MIME SoundCloud/Mixcloud.
    pub fn from_mime(mime: &str) -> Container {
        let m = mime.to_ascii_lowercase();
        if m.contains("mpeg") || m.contains("mp3") {
            Container::Mp3
        } else if m.contains("mp4") || m.contains("m4a") || m.contains("aac") {
            Container::Mp4
        } else if m.contains("ogg") || m.contains("opus") || m.contains("vorbis") {
            Container::Ogg
        } else {
            Container::Unknown
        }
    }
}

/// Façon de récupérer les octets audio.
#[derive(Debug, Clone)]
pub enum StreamKind {
    /// Un seul fichier HTTP (supporte généralement les requêtes `Range`).
    Progressive(String),
    /// Liste ordonnée de segments HLS à concaténer.
    HlsSegments(Vec<String>),
}

/// Flux résolu prêt à être décodé, avec indice de conteneur.
#[derive(Debug, Clone)]
pub struct StreamSource {
    pub kind: StreamKind,
    pub container: Container,
}

/// Erreurs de la couche providers.
#[derive(Debug)]
pub enum ProviderError {
    Http(HttpError),
    /// URL non reconnue comme SoundCloud/Mixcloud.
    Unsupported(String),
    /// Réponse de l'API mal formée ou champ attendu manquant.
    Malformed(String),
    /// Contenu présent mais non récupérable (preview only, exclusif, DRM…).
    Unavailable(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Http(e) => write!(f, "{e}"),
            ProviderError::Unsupported(u) => write!(f, "URL non supportée : {u}"),
            ProviderError::Malformed(m) => write!(f, "réponse inattendue : {m}"),
            ProviderError::Unavailable(m) => write!(f, "indisponible : {m}"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<HttpError> for ProviderError {
    fn from(e: HttpError) -> Self {
        ProviderError::Http(e)
    }
}

/// Détecte la plateforme d'une URL.
pub fn platform_of(url: &str) -> Option<Platform> {
    let u = url.to_ascii_lowercase();
    if u.contains("soundcloud.com") || u.contains("snd.sc") {
        Some(Platform::SoundCloud)
    } else if u.contains("mixcloud.com") {
        Some(Platform::Mixcloud)
    } else {
        None
    }
}

/// Résout une URL publique vers (métadonnées, flux jouable), toute plateforme.
pub fn resolve_url(agent: &ureq::Agent, url: &str) -> Result<(Track, StreamSource), ProviderError> {
    match platform_of(url) {
        Some(Platform::SoundCloud) => soundcloud::resolve(agent, url),
        Some(Platform::Mixcloud) => mixcloud::resolve(agent, url),
        None => Err(ProviderError::Unsupported(url.to_string())),
    }
}

/// Recherche unifiée : interroge les deux plateformes et entrelace les
/// résultats (best-effort — l'échec d'une plateforme n'empêche pas l'autre).
pub fn search_all(agent: &ureq::Agent, query: &str, per_platform: u32) -> Vec<Track> {
    let sc = soundcloud::search(agent, query, per_platform).unwrap_or_default();
    let mc = mixcloud::search(agent, query, per_platform).unwrap_or_default();
    interleave(sc, mc)
}

/// Alterne les éléments de deux listes (a0, b0, a1, b1, …).
fn interleave(a: Vec<Track>, b: Vec<Track>) -> Vec<Track> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut ai, mut bi) = (a.into_iter(), b.into_iter());
    loop {
        match (ai.next(), bi.next()) {
            (Some(x), Some(y)) => {
                out.push(x);
                out.push(y);
            }
            (Some(x), None) => out.push(x),
            (None, Some(y)) => out.push(y),
            (None, None) => break,
        }
    }
    out
}

/// Section de bibliothèque liée à un compte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySection {
    Likes,
    Playlists,
    Feed,
}

/// Charge une section de la bibliothèque de l'utilisateur (données publiques),
/// en fusionnant SoundCloud et Mixcloud. Best-effort : une plateforme en échec
/// ou non configurée n'empêche pas l'autre.
pub fn library(
    agent: &ureq::Agent,
    sc_handle: Option<&str>,
    mc_handle: Option<&str>,
    section: LibrarySection,
) -> Vec<Track> {
    let (mut sc, mut mc) = (Vec::new(), Vec::new());
    match section {
        LibrarySection::Likes => {
            if let Some(h) = sc_handle {
                sc = soundcloud::user_likes(agent, h, 50).unwrap_or_default();
            }
            if let Some(h) = mc_handle {
                mc = mixcloud::user_favorites(agent, h, 50).unwrap_or_default();
            }
        }
        LibrarySection::Playlists => {
            if let Some(h) = sc_handle {
                sc = soundcloud::user_playlist_tracks(agent, h, 20).unwrap_or_default();
            }
            if let Some(h) = mc_handle {
                mc = mixcloud::user_playlist_tracks(agent, h, 5).unwrap_or_default();
            }
        }
        LibrarySection::Feed => {
            // Le fil SoundCloud exige OAuth : on utilise l'historique d'écoutes
            // Mixcloud comme « fil » côté public.
            if let Some(h) = mc_handle {
                mc = mixcloud::user_listens(agent, h, 50).unwrap_or_default();
            }
        }
    }
    interleave(sc, mc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrelace_deux_listes() {
        let mk = |p, n: &str| Track {
            platform: p,
            id: n.into(),
            title: n.into(),
            artist: "a".into(),
            permalink: "u".into(),
            duration_ms: None,
        };
        let a = vec![
            mk(Platform::SoundCloud, "s1"),
            mk(Platform::SoundCloud, "s2"),
        ];
        let b = vec![mk(Platform::Mixcloud, "m1")];
        let r = interleave(a, b);
        let titles: Vec<_> = r.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["s1", "m1", "s2"]);
    }

    #[test]
    fn detecte_la_plateforme() {
        assert_eq!(
            platform_of("https://soundcloud.com/a/b"),
            Some(Platform::SoundCloud)
        );
        assert_eq!(
            platform_of("https://www.mixcloud.com/a/b/"),
            Some(Platform::Mixcloud)
        );
        assert_eq!(platform_of("https://example.com"), None);
    }

    #[test]
    fn conteneur_depuis_mime() {
        assert_eq!(Container::from_mime("audio/mpeg"), Container::Mp3);
        assert_eq!(
            Container::from_mime("audio/mp4; codecs=\"mp4a.40.2\""),
            Container::Mp4
        );
        assert_eq!(
            Container::from_mime("audio/ogg; codecs=\"opus\""),
            Container::Ogg
        );
    }
}
