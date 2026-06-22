//! Provider SoundCloud : résolution de flux via l'API publique `api-v2` et un
//! `client_id` scrapé depuis le site (mode sans compte).
//!
//! Pipeline : client_id → `/resolve` → choix transcoding → URL signée →
//! progressive (mp3 direct) ou HLS (segments).

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::{hls, Container, ProviderError, StreamKind, StreamSource};
use crate::http::HttpError;
use crate::model::{Platform, Track};

const API: &str = "https://api-v2.soundcloud.com";

thread_local! {
    /// Cache mémoire du client_id pour éviter de re-scraper à chaque appel.
    static CLIENT_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("waveline").join("sc_client_id"))
}

/// Renvoie un client_id valide : mémoire → disque → scraping du site.
pub fn client_id(agent: &ureq::Agent) -> Result<String, ProviderError> {
    if let Some(id) = CLIENT_ID.with(|c| c.borrow().clone()) {
        return Ok(id);
    }
    if let Some(p) = cache_path() {
        if let Ok(s) = fs::read_to_string(&p) {
            let s = s.trim().to_string();
            if s.len() >= 16 {
                CLIENT_ID.with(|c| *c.borrow_mut() = Some(s.clone()));
                return Ok(s);
            }
        }
    }
    let id = scrape_client_id(agent)?;
    store_client_id(&id);
    Ok(id)
}

/// Invalide le client_id en cache (à appeler sur 401/403) pour forcer un re-scrap.
pub fn invalidate_client_id() {
    CLIENT_ID.with(|c| *c.borrow_mut() = None);
    if let Some(p) = cache_path() {
        let _ = fs::remove_file(p);
    }
}

fn store_client_id(id: &str) {
    CLIENT_ID.with(|c| *c.borrow_mut() = Some(id.to_string()));
    if let Some(p) = cache_path() {
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(p, id);
    }
}

/// Scrape un client_id depuis les bundles JS de soundcloud.com.
fn scrape_client_id(agent: &ureq::Agent) -> Result<String, ProviderError> {
    let html = agent
        .get("https://soundcloud.com/")
        .call()
        .map_err(HttpError::from)?
        .into_string()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;

    // Les bundles contenant le client_id sont en général les derniers.
    let mut scripts = find_script_urls(&html);
    scripts.reverse();
    for url in scripts {
        let js = match agent.get(&url).call() {
            Ok(r) => r.into_string().unwrap_or_default(),
            Err(_) => continue,
        };
        if let Some(id) = extract_client_id(&js) {
            return Ok(id);
        }
    }
    Err(ProviderError::Malformed(
        "client_id introuvable dans les bundles SoundCloud".into(),
    ))
}

/// Extrait les URLs des `<script src="...">` pointant vers les assets sndcdn.
fn find_script_urls(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in html.split("<script").skip(1) {
        if let Some(src) = attr_value(part, "src") {
            if src.contains("sndcdn.com") && src.ends_with(".js") {
                out.push(src);
            }
        }
    }
    out
}

/// Lit la valeur d'un attribut HTML `name="..."` dans un fragment.
fn attr_value(fragment: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = fragment.find(&needle)? + needle.len();
    let rest = &fragment[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Cherche `client_id:"<alnum 32>"` (minifié) dans un bundle JS.
fn extract_client_id(js: &str) -> Option<String> {
    let mut from = 0;
    while let Some(rel) = js[from..].find("client_id") {
        let i = from + rel + "client_id".len();
        // Saute les caractères non alphanumériques (`:`, `=`, `"`, espaces).
        let bytes = js.as_bytes();
        let mut j = i;
        while j < bytes.len() && !bytes[j].is_ascii_alphanumeric() {
            j += 1;
        }
        let start = j;
        while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
            j += 1;
        }
        let candidate = &js[start..j];
        if candidate.len() == 32 {
            return Some(candidate.to_string());
        }
        from = i;
    }
    None
}

/// Recherche de morceaux (mode public, sans compte).
pub fn search(
    agent: &ureq::Agent,
    query: &str,
    limit: u32,
) -> Result<Vec<Track>, ProviderError> {
    let cid = client_id(agent)?;
    let v: Value = agent
        .get(&format!("{API}/search/tracks"))
        .query("q", query)
        .query("client_id", &cid)
        .query("limit", &limit.to_string())
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    let items = v
        .get("collection")
        .and_then(|c| c.as_array())
        .ok_or_else(|| ProviderError::Malformed("recherche sans collection".into()))?;
    Ok(items
        .iter()
        .filter(|t| t.get("kind").and_then(|k| k.as_str()) == Some("track"))
        .filter_map(|t| track_from_json(t).ok())
        .collect())
}

/// Résout un profil `soundcloud.com/<handle>` vers son identifiant numérique.
pub fn resolve_user_id(agent: &ureq::Agent, handle: &str) -> Result<i64, ProviderError> {
    let cid = client_id(agent)?;
    let url = format!("https://soundcloud.com/{}", handle.trim_matches('/'));
    let v: Value = agent
        .get(&format!("{API}/resolve"))
        .query("url", &url)
        .query("client_id", &cid)
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    if v.get("kind").and_then(|k| k.as_str()) != Some("user") {
        return Err(ProviderError::Unavailable(format!(
            "« {handle} » n'est pas un profil SoundCloud"
        )));
    }
    v.get("id")
        .and_then(|i| i.as_i64())
        .ok_or_else(|| ProviderError::Malformed("profil sans id".into()))
}

/// Likes publics d'un utilisateur (morceaux). `limit` ≤ 200.
pub fn user_likes(agent: &ureq::Agent, handle: &str, limit: u32) -> Result<Vec<Track>, ProviderError> {
    let id = resolve_user_id(agent, handle)?;
    let cid = client_id(agent)?;
    let v: Value = agent
        .get(&format!("{API}/users/{id}/track_likes"))
        .query("client_id", &cid)
        .query("limit", &limit.to_string())
        .query("linked_partitioning", "1")
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    Ok(collection_tracks(&v))
}

/// Playlists publiques d'un utilisateur, aplaties en morceaux.
pub fn user_playlist_tracks(
    agent: &ureq::Agent,
    handle: &str,
    limit: u32,
) -> Result<Vec<Track>, ProviderError> {
    let id = resolve_user_id(agent, handle)?;
    let cid = client_id(agent)?;
    let v: Value = agent
        .get(&format!("{API}/users/{id}/playlists"))
        .query("client_id", &cid)
        .query("limit", &limit.to_string())
        .query("linked_partitioning", "1")
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    let mut out = Vec::new();
    if let Some(playlists) = v.get("collection").and_then(|c| c.as_array()) {
        for p in playlists {
            if let Some(tracks) = p.get("tracks").and_then(|t| t.as_array()) {
                for t in tracks {
                    if let Ok(track) = track_from_json(t) {
                        out.push(track);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Extrait les morceaux d'une collection api-v2 (items directs ou `{track:…}`).
fn collection_tracks(v: &Value) -> Vec<Track> {
    let Some(items) = v.get("collection").and_then(|c| c.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|it| {
            let t = it.get("track").unwrap_or(it);
            track_from_json(t).ok()
        })
        .collect()
}

/// Résout une URL SoundCloud vers (Track, flux jouable).
pub fn resolve(agent: &ureq::Agent, url: &str) -> Result<(Track, StreamSource), ProviderError> {
    let cid = client_id(agent)?;
    let track_json = match resolve_json(agent, url, &cid) {
        Err(ProviderError::Http(HttpError::Status(401 | 403, _))) => {
            // client_id périmé : on réessaie une fois après re-scrap.
            invalidate_client_id();
            let cid2 = client_id(agent)?;
            resolve_json(agent, url, &cid2)?
        }
        other => other?,
    };
    let cid = client_id(agent)?;
    let track = track_from_json(&track_json)?;
    let source = pick_stream(agent, &track_json, &cid)?;
    Ok((track, source))
}

fn resolve_json(agent: &ureq::Agent, url: &str, cid: &str) -> Result<Value, ProviderError> {
    let v: Value = agent
        .get(&format!("{API}/resolve"))
        .query("url", url)
        .query("client_id", cid)
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    if v.get("media").is_none() && v.get("kind").and_then(|k| k.as_str()) != Some("track") {
        return Err(ProviderError::Unavailable(
            "l'URL ne pointe pas vers un morceau jouable".into(),
        ));
    }
    Ok(v)
}

/// Construit le modèle unifié depuis le JSON track SoundCloud.
pub fn track_from_json(v: &Value) -> Result<Track, ProviderError> {
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .ok_or_else(|| ProviderError::Malformed("titre manquant".into()))?
        .to_string();
    let artist = v
        .get("user")
        .and_then(|u| u.get("username"))
        .and_then(|n| n.as_str())
        .unwrap_or("Inconnu")
        .to_string();
    let id = v
        .get("urn")
        .and_then(|u| u.as_str())
        .map(|s| s.to_string())
        .or_else(|| v.get("id").and_then(|i| i.as_i64()).map(|i| i.to_string()))
        .unwrap_or_default();
    let permalink = v
        .get("permalink_url")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let duration_ms = v.get("duration").and_then(|d| d.as_u64());
    Ok(Track {
        platform: Platform::SoundCloud,
        id,
        title,
        artist,
        permalink,
        duration_ms,
    })
}

/// Choisit un flux jouable parmi les transcodings, avec repli.
///
/// Les variantes chiffrées (DRM : `cbc/ctr-encrypted-hls`) sont écartées —
/// indéchiffrables. On essaie les candidats jouables par score décroissant ;
/// si la signature de l'un échoue (souvent 404 pour les titres monétisés), on
/// passe au suivant. Si seules des variantes DRM existent, on le signale.
fn pick_stream(
    agent: &ureq::Agent,
    track: &Value,
    cid: &str,
) -> Result<StreamSource, ProviderError> {
    let transcodings = track
        .get("media")
        .and_then(|m| m.get("transcodings"))
        .and_then(|t| t.as_array())
        .ok_or_else(|| ProviderError::Unavailable("aucun flux disponible".into()))?;

    let mut candidates: Vec<(i32, &Value)> = Vec::new();
    let mut has_drm = false;
    for t in transcodings {
        let proto = t
            .pointer("/format/protocol")
            .and_then(|p| p.as_str())
            .unwrap_or("");
        if proto.contains("encrypted") {
            has_drm = true; // SAMPLE-AES Widevine/PlayReady : non lisible
            continue;
        }
        candidates.push((score_transcoding(t), t));
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    let mut last_err = None;
    for (_, t) in candidates {
        match try_transcoding(agent, track, cid, t) {
            Ok(src) => return Ok(src),
            Err(e) => last_err = Some(e),
        }
    }

    if has_drm {
        Err(ProviderError::Unavailable(
            "titre protégé (DRM SoundCloud) — non lisible".into(),
        ))
    } else {
        Err(last_err.unwrap_or_else(|| ProviderError::Unavailable("aucun flux jouable".into())))
    }
}

/// Tente de signer puis construire le flux d'une transcoding donnée.
fn try_transcoding(
    agent: &ureq::Agent,
    track: &Value,
    cid: &str,
    t: &Value,
) -> Result<StreamSource, ProviderError> {
    let endpoint = t
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| ProviderError::Malformed("transcoding sans url".into()))?;
    let mime = t
        .pointer("/format/mime_type")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let protocol = t
        .pointer("/format/protocol")
        .and_then(|p| p.as_str())
        .unwrap_or("progressive");
    let container = Container::from_mime(mime);

    // Signe l'URL (valable quelques minutes). `track_authorization` requis.
    let mut req = agent.get(endpoint).query("client_id", cid);
    if let Some(auth) = track.get("track_authorization").and_then(|a| a.as_str()) {
        req = req.query("track_authorization", auth);
    }
    let signed: Value = req
        .call()
        .map_err(HttpError::from)?
        .into_json()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    let stream_url = signed
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| ProviderError::Malformed("url signée manquante".into()))?
        .to_string();

    if protocol == "progressive" {
        Ok(StreamSource {
            kind: StreamKind::Progressive(stream_url),
            container,
        })
    } else {
        let segments = expand_hls(agent, &stream_url)?;
        Ok(StreamSource {
            kind: StreamKind::HlsSegments(segments),
            container,
        })
    }
}

/// Note une transcoding : progressive mp3 > hls mp3 > hls aac > opus ; les
/// previews (snipped) sont fortement pénalisées.
fn score_transcoding(t: &Value) -> i32 {
    let proto = t
        .pointer("/format/protocol")
        .and_then(|p| p.as_str())
        .unwrap_or("");
    let mime = t
        .pointer("/format/mime_type")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let preset = t.get("preset").and_then(|p| p.as_str()).unwrap_or("");
    let snipped = t.get("snipped").and_then(|s| s.as_bool()).unwrap_or(false);

    let mut score = 0;
    if proto == "progressive" {
        score += 100;
    } else if proto == "hls" {
        score += 50;
    }
    if mime.contains("mpeg") || preset.contains("mp3") {
        score += 30;
    } else if mime.contains("mp4") || mime.contains("aac") || preset.contains("aac") {
        score += 20;
    } else if mime.contains("opus") {
        score += 5; // décodage opus non géré pour l'instant
    }
    if snipped {
        score -= 200;
    }
    score
}

/// Télécharge et déplie un m3u8 (un niveau de master) vers la liste de segments.
fn expand_hls(agent: &ureq::Agent, m3u8_url: &str) -> Result<Vec<String>, ProviderError> {
    let text = agent
        .get(m3u8_url)
        .call()
        .map_err(HttpError::from)?
        .into_string()
        .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
    if hls::is_encrypted(&text) {
        return Err(ProviderError::Unavailable(
            "flux HLS chiffré (AES-128) non supporté".into(),
        ));
    }
    match hls::parse(&text, m3u8_url) {
        hls::Playlist::Media(segs) => Ok(segs),
        hls::Playlist::Master(variants) => {
            let first = variants
                .first()
                .ok_or_else(|| ProviderError::Malformed("master m3u8 vide".into()))?;
            let text2 = agent
                .get(first)
                .call()
                .map_err(HttpError::from)?
                .into_string()
                .map_err(|e| ProviderError::Http(HttpError::Decode(e.to_string())))?;
            match hls::parse(&text2, first) {
                hls::Playlist::Media(segs) => Ok(segs),
                hls::Playlist::Master(_) => Err(ProviderError::Malformed(
                    "master m3u8 imbriqué".into(),
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extrait_client_id_minifie() {
        let js = r#"...,client_id:"abcDEF1234567890abcDEF1234567890",api:..."#;
        assert_eq!(
            extract_client_id(js).as_deref(),
            Some("abcDEF1234567890abcDEF1234567890")
        );
    }

    #[test]
    fn ignore_client_id_trop_court() {
        let js = r#"client_id:"tropcourt""#;
        assert_eq!(extract_client_id(js), None);
    }

    #[test]
    fn trouve_les_scripts_sndcdn() {
        let html = r#"<script crossorigin src="https://a-v2.sndcdn.com/assets/0-abc.js"></script>
            <script src="https://other.com/x.js"></script>
            <script src="https://a-v2.sndcdn.com/assets/9-zzz.js"></script>"#;
        let urls = find_script_urls(html);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("0-abc.js"));
    }

    #[test]
    fn score_prefere_progressive_mp3() {
        let prog_mp3 = json!({"format":{"protocol":"progressive","mime_type":"audio/mpeg"},"preset":"mp3_0_0"});
        let hls_aac = json!({"format":{"protocol":"hls","mime_type":"audio/mp4"},"preset":"aac_160k"});
        let snippet = json!({"format":{"protocol":"progressive","mime_type":"audio/mpeg"},"snipped":true});
        assert!(score_transcoding(&prog_mp3) > score_transcoding(&hls_aac));
        assert!(score_transcoding(&hls_aac) > score_transcoding(&snippet));
    }

    #[test]
    fn track_depuis_json_minimal() {
        let v = json!({
            "title": "Kerala",
            "user": {"username": "Bonobo"},
            "id": 12345,
            "permalink_url": "https://soundcloud.com/bonobo/kerala",
            "duration": 290000
        });
        let t = track_from_json(&v).unwrap();
        assert_eq!(t.title, "Kerala");
        assert_eq!(t.artist, "Bonobo");
        assert_eq!(t.duration_ms, Some(290000));
        assert_eq!(t.id, "12345");
    }
}
