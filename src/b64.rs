//! Décodeur base64 standard minimal (évite la dépendance `base64`).
//!
//! Utilisé pour déchiffrer les URLs de flux Mixcloud (base64 puis XOR).

const PAD: u8 = b'=';

/// Décode une chaîne base64 standard (alphabet `A-Za-z0-9+/`, padding `=`).
/// Ignore les espaces/retours à la ligne. Retourne `None` si invalide.
pub fn decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buf = 0u32;
    let mut bits = 0u8;
    for &c in input.as_bytes() {
        if c == PAD {
            break;
        }
        let val = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_cas_standards() {
        assert_eq!(decode("").unwrap(), b"");
        assert_eq!(decode("Zg==").unwrap(), b"f");
        assert_eq!(decode("Zm8=").unwrap(), b"fo");
        assert_eq!(decode("Zm9v").unwrap(), b"foo");
        assert_eq!(decode("aGVsbG8gd29ybGQ=").unwrap(), b"hello world");
    }

    #[test]
    fn ignore_les_espaces() {
        assert_eq!(decode("Zm9v\nYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn refuse_caracteres_invalides() {
        assert!(decode("****").is_none());
    }
}
