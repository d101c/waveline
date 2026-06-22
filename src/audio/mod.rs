//! Moteur audio : décodage 100% Rust (symphonia) et sortie via un lecteur PCM
//! système. L'UI ne voit que [`Player`].

mod player;
mod sink;
mod source;
pub mod spectrum;

pub use player::{Player, Shared};
