//! Sources d'octets pour le décodeur : flux HTTP progressif (seekable via les
//! requêtes Range) et flux HLS (segments concaténés à la volée). Toutes deux
//! implémentent [`symphonia_core::io::MediaSource`].
//!
//! La seekabilité du flux progressif est indispensable pour le MP4/M4A dont
//! l'atome `moov` est en fin de fichier (« missing moov atom ») : symphonia
//! doit pouvoir sauter à la fin pour le lire, puis revenir.

use std::io::{self, Read, Seek, SeekFrom};

use symphonia::core::io::MediaSource;

use crate::http::HttpError;

/// Flux HTTP progressif, seekable par requêtes `Range`.
///
/// Lecture séquentielle par défaut ; un `seek` repositionne `pos` et rouvre une
/// connexion `Range: bytes=pos-` à la prochaine lecture.
pub struct HttpStream {
    agent: ureq::Agent,
    url: String,
    len: Option<u64>,
    pos: u64,
    reader: Option<Box<dyn Read + Send + Sync>>,
}

impl HttpStream {
    pub fn open(agent: &ureq::Agent, url: &str) -> Result<HttpStream, HttpError> {
        let resp = agent.get(url).call()?;
        let len = resp
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok());
        Ok(HttpStream {
            agent: agent.clone(),
            url: url.to_string(),
            len,
            pos: 0,
            reader: Some(resp.into_reader()),
        })
    }

    /// (Ré)ouvre une connexion à partir de `self.pos` via une requête Range.
    fn open_at(&mut self) -> io::Result<()> {
        let resp = self
            .agent
            .get(&self.url)
            .set("Range", &format!("bytes={}-", self.pos))
            .call()
            .map_err(|e| io::Error::other(e.to_string()))?;
        let partial = resp.status() == 206;
        let mut reader = resp.into_reader();
        // Si le serveur ignore Range (200 au lieu de 206), on saute `pos` octets.
        if !partial && self.pos > 0 {
            io::copy(&mut (&mut reader).take(self.pos), &mut io::sink())?;
        }
        self.reader = Some(reader);
        Ok(())
    }
}

impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.reader.is_none() {
            self.open_at()?;
        }
        let n = self.reader.as_mut().unwrap().read(buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HttpStream {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(p) => p,
            SeekFrom::End(off) => {
                let len = self.len.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Unsupported, "longueur inconnue")
                })?;
                (len as i64 + off).max(0) as u64
            }
            SeekFrom::Current(off) => (self.pos as i64 + off).max(0) as u64,
        };
        if target != self.pos {
            self.pos = target;
            self.reader = None; // réouverture paresseuse à la prochaine lecture
        }
        Ok(self.pos)
    }
}

impl MediaSource for HttpStream {
    fn is_seekable(&self) -> bool {
        self.len.is_some()
    }
    fn byte_len(&self) -> Option<u64> {
        self.len
    }
}

/// Flux HLS : enchaîne les segments en un seul flux d'octets continu.
pub struct HlsReader {
    agent: ureq::Agent,
    segments: Vec<String>,
    idx: usize,
    current: Option<Box<dyn Read + Send + Sync>>,
}

impl HlsReader {
    pub fn new(agent: ureq::Agent, segments: Vec<String>) -> HlsReader {
        HlsReader {
            agent,
            segments,
            idx: 0,
            current: None,
        }
    }

    /// Ouvre le segment suivant ; renvoie `false` quand il n'y en a plus.
    fn open_next(&mut self) -> io::Result<bool> {
        if self.idx >= self.segments.len() {
            return Ok(false);
        }
        let url = &self.segments[self.idx];
        self.idx += 1;
        let resp = self
            .agent
            .get(url)
            .call()
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.current = Some(resp.into_reader());
        Ok(true)
    }
}

impl Read for HlsReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.current.is_none() && !self.open_next()? {
                return Ok(0); // fin de tous les segments
            }
            let reader = self.current.as_mut().unwrap();
            let n = reader.read(buf)?;
            if n == 0 {
                self.current = None; // segment épuisé : passe au suivant
                continue;
            }
            return Ok(n);
        }
    }
}

impl Seek for HlsReader {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "flux HLS non-seekable",
        ))
    }
}

impl MediaSource for HlsReader {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}
