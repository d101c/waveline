//! Client HTTP partagé (ureq + rustls, 100% Rust).
//!
//! Un seul agent réutilisé pour toute l'app, avec un User-Agent de navigateur
//! réaliste : SoundCloud et surtout Mixcloud filtrent les clients non-navigateur.

use std::time::Duration;

/// User-Agent Chrome réaliste, constant (cf. recommandation de résilience).
pub const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// Construit l'agent ureq partagé.
pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
}

/// Erreur réseau simplifiée pour la couche providers.
#[derive(Debug)]
pub enum HttpError {
    Status(u16, String),
    Transport(String),
    Decode(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Status(c, url) => write!(f, "HTTP {c} sur {url}"),
            HttpError::Transport(e) => write!(f, "réseau : {e}"),
            HttpError::Decode(e) => write!(f, "décodage : {e}"),
        }
    }
}

impl std::error::Error for HttpError {}

impl From<ureq::Error> for HttpError {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Status(code, resp) => {
                HttpError::Status(code, resp.get_url().to_string())
            }
            ureq::Error::Transport(t) => HttpError::Transport(t.to_string()),
        }
    }
}

impl From<std::io::Error> for HttpError {
    fn from(e: std::io::Error) -> Self {
        HttpError::Transport(e.to_string())
    }
}
