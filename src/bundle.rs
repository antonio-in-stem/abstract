//! ABX bundles: sealed, tamper-evident containers for compiled Abstract data.
//!
//! `abstract bundle` compiles a project and writes an `.abx` file that ships
//! inside an application (for example a Java plugin jar). The payload is the
//! compiled JSON document encrypted with ChaCha20-Poly1305, so the authored
//! data never appears as plain text inside the artifact and any modification
//! is detected when the bundle is opened.
//!
//! Layout (little-endian):
//!
//! ```text
//! offset  size  field
//! 0       4     magic "ABX1"
//! 4       1     flags (bit 0: encrypted)
//! 5       1     payload format (1 = compiled JSON document)
//! 6       12    nonce (zero when the bundle is plain)
//! 18      4     payload length in bytes (u32)
//! 22      n     payload (ciphertext when encrypted, plus a 16-byte tag)
//! ```
//!
//! The 22-byte header is authenticated as associated data, so flags and
//! lengths cannot be altered without breaking the tag.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::{open, seal, sha256};

static SEAL_COUNTER: AtomicU64 = AtomicU64::new(0);

/// File signature for bundle version 1.
pub const MAGIC: [u8; 4] = *b"ABX1";
/// Flag bit marking an encrypted payload.
pub const FLAG_ENCRYPTED: u8 = 0b0000_0001;
/// Payload format identifier for the compiled JSON document.
pub const FORMAT_JSON: u8 = 1;

const HEADER_LENGTH: usize = 22;
const TAG_LENGTH: usize = 16;

/// Derives a 32-byte bundle key from CLI key material.
///
/// - `hex:<64 hex digits>` uses the exact key bytes.
/// - anything else is hashed with SHA-256, so passphrases of any length work.
pub fn derive_key(material: &str) -> Result<[u8; 32], String> {
    if let Some(hex_part) = material.strip_prefix("hex:") {
        let digits: Vec<char> = hex_part.chars().collect();
        if digits.len() != 64 || !digits.iter().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(
                "hex keys must contain exactly 64 hexadecimal digits after 'hex:'".to_string(),
            );
        }
        let mut key = [0u8; 32];
        for (index, pair) in hex_part.as_bytes().chunks(2).enumerate() {
            let text = std::str::from_utf8(pair).map_err(|_| "invalid hex key".to_string())?;
            key[index] =
                u8::from_str_radix(text, 16).map_err(|_| "invalid hex key".to_string())?;
        }
        return Ok(key);
    }
    if material.trim().is_empty() {
        return Err("bundle keys must not be empty".to_string());
    }
    Ok(sha256(material.as_bytes()))
}

/// Encodes `payload` into an ABX bundle. When `key` is provided the payload
/// is sealed with ChaCha20-Poly1305; otherwise it is stored as plain bytes
/// (useful for debugging, not for shipping).
pub fn encode(payload: &[u8], key: Option<&[u8; 32]>) -> Vec<u8> {
    let mut header = [0u8; HEADER_LENGTH];
    header[..4].copy_from_slice(&MAGIC);
    header[5] = FORMAT_JSON;

    match key {
        Some(key) => {
            header[4] = FLAG_ENCRYPTED;
            let nonce = derive_nonce(payload, key);
            header[6..18].copy_from_slice(&nonce);
            let sealed_length = payload.len() + TAG_LENGTH;
            header[18..22].copy_from_slice(&(sealed_length as u32).to_le_bytes());
            let sealed = seal(key, &nonce, &header, payload);
            let mut output = header.to_vec();
            output.extend_from_slice(&sealed);
            output
        }
        None => {
            header[18..22].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            let mut output = header.to_vec();
            output.extend_from_slice(payload);
            output
        }
    }
}

/// Decodes an ABX bundle back into its payload bytes. Encrypted bundles
/// require the matching key; a wrong key or a modified byte fails cleanly.
pub fn decode(bytes: &[u8], key: Option<&[u8; 32]>) -> Result<Vec<u8>, String> {
    if bytes.len() < HEADER_LENGTH {
        return Err("bundle is too small to contain an ABX header".to_string());
    }
    if bytes[..4] != MAGIC {
        return Err("not an ABX bundle (bad magic)".to_string());
    }
    let flags = bytes[4];
    if bytes[5] != FORMAT_JSON {
        return Err(format!("unsupported bundle payload format {}", bytes[5]));
    }
    let declared = u32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]) as usize;
    let body = &bytes[HEADER_LENGTH..];
    if body.len() != declared {
        return Err(format!(
            "bundle payload length mismatch: header declares {declared} bytes, found {}",
            body.len()
        ));
    }

    if flags & FLAG_ENCRYPTED == 0 {
        if key.is_some() {
            // A caller holding a key expects sealed data. Accepting a
            // "plain" bundle here would allow a downgrade attack where an
            // attacker clears the encryption flag to bypass authentication.
            return Err(
                "bundle claims to be unencrypted but a key was provided; refusing the downgrade"
                    .to_string(),
            );
        }
        return Ok(body.to_vec());
    }
    let Some(key) = key else {
        return Err("bundle is encrypted; a key is required to open it".to_string());
    };
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&bytes[6..18]);
    open(key, &nonce, &bytes[..HEADER_LENGTH], body)
        .ok_or_else(|| "bundle authentication failed (wrong key or modified data)".to_string())
}

/// Builds a unique nonce for this seal operation. Uniqueness comes from the
/// payload digest mixed with the wall clock and a process-local counter, so
/// re-bundling identical data still produces fresh ciphertext even on
/// platforms with coarse clocks.
fn derive_nonce(payload: &[u8], key: &[u8; 32]) -> [u8; 12] {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = SEAL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut material = Vec::with_capacity(96);
    material.extend_from_slice(&sha256(payload));
    material.extend_from_slice(key);
    material.extend_from_slice(&clock.to_le_bytes());
    material.extend_from_slice(&counter.to_le_bytes());
    let digest = sha256(&material);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&digest[..12]);
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_bundles_roundtrip() {
        let payload = br#"{"data": []}"#;
        let bundle = encode(payload, None);
        assert_eq!(decode(&bundle, None).unwrap(), payload);
    }

    #[test]
    fn encrypted_bundles_roundtrip_and_authenticate() {
        let payload = br#"{"data": [{"id": "atlas"}]}"#;
        let key = derive_key("sunny meadows").unwrap();
        let bundle = encode(payload, Some(&key));

        assert_eq!(decode(&bundle, Some(&key)).unwrap(), payload);
        assert!(decode(&bundle, None).is_err());

        let wrong = derive_key("cloudy meadows").unwrap();
        assert!(decode(&bundle, Some(&wrong)).is_err());

        let mut tampered = bundle.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x40;
        assert!(decode(&tampered, Some(&key)).is_err());

        let mut flag_flipped = bundle;
        flag_flipped[4] = 0;
        assert!(decode(&flag_flipped, Some(&key)).is_err());
    }

    #[test]
    fn hex_keys_are_used_verbatim() {
        let key = derive_key(
            "hex:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .unwrap();
        assert_eq!(key[0], 0x00);
        assert_eq!(key[31], 0x1f);
        assert!(derive_key("hex:abcd").is_err());
        assert!(derive_key("").is_err());
    }

    #[test]
    fn fresh_seals_use_fresh_nonces() {
        let payload = b"same payload";
        let key = derive_key("same key").unwrap();
        let first = encode(payload, Some(&key));
        let second = encode(payload, Some(&key));
        assert_ne!(first[6..18], second[6..18]);
    }
}
