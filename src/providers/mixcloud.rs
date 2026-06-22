//! Provider Mixcloud : métadonnées via l'API REST publique, flux via GraphQL
//! (`streamInfo`) dont les URLs sont base64 + XOR avec une clé en clair.

use serde_json::{json, Value};

use super::{hls, Container, ProviderError, StreamKind, StreamSource};
use crate::http::HttpError;
use crate::model::{Platform, Track};

const GRAPHQL: &str = "https://www.mixcloud.com/graphql";
const REST: &str = "https://api.mixcloud.com";

/// Recherche de cloudcasts (mode public, sans compte) via l'API REST.
pub fn search(agent: &ureq::Agent, query: &str, limit: u32) -> Result<Vec<Track>, ProviderError> {
    let v: Value = agent
        .get(&format!("{REST}/search/"))
        .query("q", query)
        .query("type", "cloudcast")
        .query("limit", &limit.to_string())
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    let items = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| ProviderError::Malformed("recherche sans data".into()))?;
    Ok(items.iter().filter_map(track_from_rest).collect())
}

/// Construit un Track depuis un objet cloudcast de l'API REST.
fn track_from_rest(c: &Value) -> Option<Track> {
    let key = c.get("key").and_then(|k| k.as_str())?; // ex: "/user/slug/"
    let title = c.get("name").and_then(|n| n.as_str())?.to_string();
    let artist = c
        .pointer("/user/name")
        .and_then(|n| n.as_str())
        .unwrap_or("Inconnu")
        .to_string();
    let duration_ms = c
        .get("audio_length")
        .and_then(|d| d.as_u64())
        .map(|s| s * 1000);
    Some(Track {
        platform: Platform::Mixcloud,
        id: key.to_string(),
        title,
        artist,
        permalink: format!("https://www.mixcloud.com{key}"),
        duration_ms,
    })
}

/// Liste de cloudcasts publique d'un utilisateur (`favorites`, `listens`, …).
fn user_list(
    agent: &ureq::Agent,
    handle: &str,
    kind: &str,
    limit: u32,
) -> Result<Vec<Track>, ProviderError> {
    let h = handle.trim_matches('/');
    let v: Value = agent
        .get(&format!("{REST}/{h}/{kind}/"))
        .query("limit", &limit.to_string())
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    let items = v.get("data").and_then(|d| d.as_array());
    Ok(items
        .map(|arr| arr.iter().filter_map(track_from_rest).collect())
        .unwrap_or_default())
}

/// Favoris publics.
pub fn user_favorites(
    agent: &ureq::Agent,
    handle: &str,
    limit: u32,
) -> Result<Vec<Track>, ProviderError> {
    user_list(agent, handle, "favorites", limit)
}

/// Historique d'écoutes public.
pub fn user_listens(
    agent: &ureq::Agent,
    handle: &str,
    limit: u32,
) -> Result<Vec<Track>, ProviderError> {
    user_list(agent, handle, "listens", limit)
}

/// Playlists publiques, aplaties en morceaux (plafonné à quelques playlists).
pub fn user_playlist_tracks(
    agent: &ureq::Agent,
    handle: &str,
    max_playlists: usize,
) -> Result<Vec<Track>, ProviderError> {
    let h = handle.trim_matches('/');
    let v: Value = agent
        .get(&format!("{REST}/{h}/playlists/"))
        .query("limit", "20")
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    let mut out = Vec::new();
    if let Some(playlists) = v.get("data").and_then(|d| d.as_array()) {
        for p in playlists.iter().take(max_playlists) {
            let Some(slug) = p.get("slug").and_then(|s| s.as_str()) else {
                continue;
            };
            let cc: Value = match agent
                .get(&format!("{REST}/{h}/playlists/{slug}/cloudcasts/"))
                .query("limit", "50")
                .call()
            {
                Ok(r) => match r.into_json() {
                    Ok(j) => j,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            if let Some(arr) = cc.get("data").and_then(|d| d.as_array()) {
                out.extend(arr.iter().filter_map(track_from_rest));
            }
        }
    }
    Ok(out)
}

/// Clé XOR (en clair) appliquée après décodage base64 des URLs de flux.
const KEY: &[u8] = b"IFYOUWANTTHEARTISTSTOGETPAIDDONOTDOWNLOADFROMMIXCLOUD";

/// Déchiffre un champ `streamInfo` : base64 puis XOR cyclique avec [`KEY`].
pub fn decrypt(field: &str) -> Option<String> {
    let raw = crate::b64::decode(field)?;
    let out: Vec<u8> = raw
        .iter()
        .zip(KEY.iter().cycle())
        .map(|(b, k)| b ^ k)
        .collect();
    String::from_utf8(out).ok()
}

/// Extrait `(username, slug)` d'une URL Mixcloud.
pub fn parse_url(url: &str) -> Option<(String, String)> {
    let after = url.split("mixcloud.com/").nth(1)?;
    let path = after.split(['?', '#']).next().unwrap_or(after);
    let mut segs = path.split('/').filter(|s| !s.is_empty());
    let user = segs.next()?.to_string();
    let slug = segs.next()?.to_string();
    // Évite de confondre /discover, /upload, etc. (heuristique légère).
    if user.is_empty() || slug.is_empty() {
        return None;
    }
    Some((user, slug))
}

fn container_from_url(url: &str) -> Container {
    let u = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    // Mixcloud sert du MP3 progressif rarement ; le reste (m4a/mp4, segments
    // HLS .m3u8) est de l'AAC en conteneur MP4.
    if u.ends_with(".mp3") {
        Container::Mp3
    } else {
        Container::Mp4
    }
}

/// Résout une URL Mixcloud vers (Track, flux jouable), en un seul appel GraphQL.
pub fn resolve(agent: &ureq::Agent, url: &str) -> Result<(Track, StreamSource), ProviderError> {
    let (user, slug) = parse_url(url).ok_or_else(|| ProviderError::Unsupported(url.to_string()))?;
    let cc = query_cloudcast(agent, &user, &slug)?;
    if let Some(reason) = cc.get("restrictedReason").and_then(|r| r.as_str()) {
        return Err(ProviderError::Unavailable(format!(
            "contenu restreint Mixcloud ({reason})"
        )));
    }
    let track = track_from_cc(&cc, &user, &slug);
    let source = stream_from_cc(agent, &cc)?;
    Ok((track, source))
}

/// Exécute la requête `cloudcastLookup` et retourne l'objet cloudcast.
fn query_cloudcast(agent: &ureq::Agent, user: &str, slug: &str) -> Result<Value, ProviderError> {
    // serde échappe correctement les guillemets de la query GraphQL inline.
    let body = json!({
        "query": format!(
            "{{cloudcastLookup(lookup:{{username:\"{user}\",slug:\"{slug}\"}}){{name owner{{displayName username}} isExclusive restrictedReason audioLength streamInfo{{url hlsUrl dashUrl}}}}}}"
        )
    });
    let resp: Value = agent
        .post(GRAPHQL)
        .set("Content-Type", "application/json")
        .set("Origin", "https://www.mixcloud.com")
        .set("Referer", "https://www.mixcloud.com/")
        .send_json(body)
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    resp.pointer("/data/cloudcastLookup")
        .filter(|c| !c.is_null())
        .cloned()
        .ok_or_else(|| ProviderError::Unavailable("cloudcast introuvable".into()))
}

/// Construit le modèle unifié depuis l'objet cloudcast GraphQL.
fn track_from_cc(cc: &Value, user: &str, slug: &str) -> Track {
    let title = cc
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or(slug)
        .to_string();
    let artist = cc
        .pointer("/owner/displayName")
        .and_then(|n| n.as_str())
        .unwrap_or(user)
        .to_string();
    let duration_ms = cc
        .get("audioLength")
        .and_then(|d| d.as_u64())
        .map(|s| s * 1000);
    Track {
        platform: Platform::Mixcloud,
        id: format!("{user}/{slug}"),
        title,
        artist,
        permalink: format!("https://www.mixcloud.com/{user}/{slug}/"),
        duration_ms,
    }
}

/// Déchiffre `streamInfo` et construit le flux jouable.
fn stream_from_cc(agent: &ureq::Agent, cc: &Value) -> Result<StreamSource, ProviderError> {
    let si = cc
        .get("streamInfo")
        .filter(|s| !s.is_null())
        .ok_or_else(|| {
            ProviderError::Unavailable("pas de streamInfo (exclusif ou supprimé)".into())
        })?;

    // Préférence : url (progressif) > hlsUrl.
    if let Some(enc) = si
        .get("url")
        .and_then(|u| u.as_str())
        .filter(|s| !s.is_empty())
    {
        if let Some(dec) = decrypt(enc) {
            return Ok(StreamSource {
                container: container_from_url(&dec),
                kind: StreamKind::Progressive(dec),
            });
        }
    }
    if let Some(enc) = si
        .get("hlsUrl")
        .and_then(|u| u.as_str())
        .filter(|s| !s.is_empty())
    {
        if let Some(dec) = decrypt(enc) {
            let segments = expand_hls(agent, &dec)?;
            return Ok(StreamSource {
                container: Container::Mp4,
                kind: StreamKind::HlsSegments(segments),
            });
        }
    }
    Err(ProviderError::Unavailable(
        "aucune URL de flux exploitable".into(),
    ))
}

fn expand_hls(agent: &ureq::Agent, m3u8_url: &str) -> Result<Vec<String>, ProviderError> {
    let text = agent
        .get(m3u8_url)
        .call()
        .map_err(HttpError::from)?
        .into_string()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    if hls::is_encrypted(&text) {
        return Err(ProviderError::Unavailable(
            "flux HLS chiffré non supporté".into(),
        ));
    }
    match hls::parse(&text, m3u8_url) {
        hls::Playlist::Media(segs) => Ok(segs),
        hls::Playlist::Master(variants) => {
            let first = variants
                .first()
                .ok_or_else(|| ProviderError::Malformed("master m3u8 vide".into()))?;
            let t2 = agent
                .get(first)
                .call()
                .map_err(HttpError::from)?
                .into_string()
                .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
            match hls::parse(&t2, first) {
                hls::Playlist::Media(segs) => Ok(segs),
                hls::Playlist::Master(_) => {
                    Err(ProviderError::Malformed("master m3u8 imbriqué".into()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_est_symetrique_et_reversible() {
        // Chiffre "https://x" puis vérifie le round-trip via base64+xor.
        let plain = b"https://stream.example/seg.m4a";
        let enc: Vec<u8> = plain
            .iter()
            .zip(KEY.iter().cycle())
            .map(|(b, k)| b ^ k)
            .collect();
        // encode base64 standard "à la main" pour le test
        let b64 = base64_encode(&enc);
        assert_eq!(
            decrypt(&b64).as_deref(),
            Some(std::str::from_utf8(plain).unwrap())
        );
    }

    #[test]
    fn parse_url_extrait_user_et_slug() {
        assert_eq!(
            parse_url("https://www.mixcloud.com/NTSRadio/the-mix/"),
            Some(("NTSRadio".into(), "the-mix".into()))
        );
        assert_eq!(
            parse_url("https://www.mixcloud.com/gilles/show/?utm=x"),
            Some(("gilles".into(), "show".into()))
        );
        assert_eq!(parse_url("https://www.mixcloud.com/onlyuser/"), None);
    }

    #[test]
    fn container_devine_le_format() {
        assert_eq!(container_from_url("https://x/seg.mp3?t=1"), Container::Mp3);
        assert_eq!(container_from_url("https://x/seg.m4a"), Container::Mp4);
    }

    /// Mini-encodeur base64 standard, réservé aux tests.
    fn base64_encode(data: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(A[((n >> 18) & 63) as usize] as char);
            out.push(A[((n >> 12) & 63) as usize] as char);
            if chunk.len() > 1 {
                out.push(A[((n >> 6) & 63) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(A[(n & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}
