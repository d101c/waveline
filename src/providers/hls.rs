//! Parsing minimal de playlists HLS (m3u8) — juste ce qu'il faut pour lister
//! les segments média dans l'ordre. Pas de support multivariant complet : on
//! gère le cas « media playlist » (segments) et on suit un seul niveau de
//! « master playlist » (on prend le premier flux listé).

/// Une playlist HLS analysée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Playlist {
    /// Playlist média : URIs de segments dans l'ordre.
    Media(Vec<String>),
    /// Playlist maître : URIs de sous-playlists (variantes de qualité).
    Master(Vec<String>),
}

/// Indique si la playlist est chiffrée (AES-128). Non supporté pour l'instant.
pub fn is_encrypted(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim();
        l.starts_with("#EXT-X-KEY") && !l.contains("METHOD=NONE")
    })
}

/// Analyse un m3u8 et résout les URIs relatives par rapport à `base`.
pub fn parse(text: &str, base: &str) -> Playlist {
    let is_master = text.contains("#EXT-X-STREAM-INF");
    let mut uris = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        uris.push(resolve_uri(base, line));
    }
    if is_master {
        Playlist::Master(uris)
    } else {
        Playlist::Media(uris)
    }
}

/// Résout une URI éventuellement relative contre l'URL de la playlist.
pub fn resolve_uri(base: &str, uri: &str) -> String {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return uri.to_string();
    }
    // URI absolue depuis la racine (« /path »).
    if let Some(rest) = uri.strip_prefix('/') {
        if let Some(origin) = origin_of(base) {
            return format!("{origin}/{rest}");
        }
    }
    // URI relative : on remplace le dernier segment de chemin du base.
    match base.rfind('/') {
        Some(i) => format!("{}/{}", &base[..i], uri),
        None => uri.to_string(),
    }
}

/// Extrait `scheme://host` d'une URL (sans le chemin), en ignorant la query.
fn origin_of(url: &str) -> Option<String> {
    let scheme_end = url.find("://")? + 3;
    let rest = &url[scheme_end..];
    let host_end = rest.find('/').unwrap_or(rest.len());
    Some(format!("{}{}", &url[..scheme_end], &rest[..host_end]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://cf-hls-media.sndcdn.com/media/0/30/playlist.m3u8?token=x";

    #[test]
    fn detecte_media_vs_master() {
        let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=128000\nchunk.m3u8\n";
        assert!(matches!(parse(master, BASE), Playlist::Master(_)));

        let media = "#EXTM3U\n#EXTINF:10,\nseg0.ts\n#EXTINF:10,\nseg1.ts\n";
        match parse(media, BASE) {
            Playlist::Media(segs) => assert_eq!(segs.len(), 2),
            _ => panic!("attendu media"),
        }
    }

    #[test]
    fn resout_uris_relatives_absolues_et_completes() {
        // relative au chemin
        assert_eq!(
            resolve_uri(BASE, "seg0.ts"),
            "https://cf-hls-media.sndcdn.com/media/0/30/seg0.ts"
        );
        // absolue depuis la racine
        assert_eq!(
            resolve_uri(BASE, "/x/seg0.ts"),
            "https://cf-hls-media.sndcdn.com/x/seg0.ts"
        );
        // déjà complète
        assert_eq!(resolve_uri(BASE, "https://h/s.ts"), "https://h/s.ts");
    }

    #[test]
    fn detecte_chiffrement() {
        assert!(is_encrypted(
            "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\nseg0.ts\n"
        ));
        assert!(!is_encrypted("#EXTM3U\n#EXT-X-KEY:METHOD=NONE\nseg0.ts\n"));
        assert!(!is_encrypted("#EXTM3U\nseg0.ts\n"));
    }
}
