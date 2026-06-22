//! Sources d'octets pour le décodeur : flux HTTP progressif (un fichier) et
//! flux HLS (segments concaténés à la volée). Toutes deux implémentent
//! [`symphonia_core::io::MediaSource`] en mode non-seekable (lecture en avant
//! uniquement), ce qui suffit aux formats de streaming MP3/AAC.

use std::io::{self, Read, Seek, SeekFrom};

use symphonia::core::io::MediaSource;

use crate::http::HttpError;

/// Flux HTTP progressif : on lit le corps de la réponse au fil de l'eau.
pub struct HttpStream {
    reader: Box<dyn Read + Send + Sync>,
    len: Option<u64>,
}

impl HttpStream {
    pub fn open(agent: &ureq::Agent, url: &str) -> Result<HttpStream, HttpError> {
        let resp = agent.get(url).call()?;
        let len = resp
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok());
        Ok(HttpStream {
            reader: resp.into_reader(),
            len,
        })
    }
}

impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

impl Seek for HttpStream {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "flux HTTP non-seekable",
        ))
    }
}

impl MediaSource for HttpStream {
    fn is_seekable(&self) -> bool {
        false
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
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
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
