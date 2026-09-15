//! Cryptographic primitives used by Abstract bundles.
//!
//! SHA-256 and ChaCha20-Poly1305 are provided by the maintained RustCrypto
//! implementations. The small wrappers in this module preserve Abstract's
//! existing byte-oriented API and ABX1 wire format.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use sha2::{Digest, Sha256};

/// Computes the SHA-256 digest of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Encrypts `plaintext` and returns `ciphertext || 16-byte tag`.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let cipher =
        ChaCha20Poly1305::new_from_slice(key).expect("ChaCha20-Poly1305 accepts every 32-byte key");
    let nonce =
        Nonce::try_from(nonce.as_slice()).expect("ChaCha20-Poly1305 accepts every 12-byte nonce");
    cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .expect("ChaCha20-Poly1305 plaintext exceeds its per-message limit")
}

/// Verifies and decrypts `ciphertext || tag`. Returns `None` when the tag
/// does not authenticate (wrong key, wrong nonce, or tampered data).
pub fn open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).ok()?;
    let nonce = Nonce::try_from(nonce.as_slice()).ok()?;
    cipher.decrypt(&nonce, Payload { msg: data, aad }).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        let clean: String = text.chars().filter(|ch| ch.is_ascii_hexdigit()).collect();
        clean
            .as_bytes()
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn sha256_matches_nist_vectors() {
        assert_eq!(
            to_hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            to_hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            to_hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn aead_matches_rfc_8439_vector() {
        // RFC 8439 section 2.8.2.
        let key: [u8; 32] = hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
            .try_into()
            .unwrap();
        let nonce: [u8; 12] = hex("070000004041424344454647").try_into().unwrap();
        let aad = hex("50515253c0c1c2c3c4c5c6c7");
        let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you \
only one tip for the future, sunscreen would be it.";

        let sealed = seal(&key, &nonce, &aad, plaintext);
        let (ciphertext, tag) = sealed.split_at(sealed.len() - 16);

        assert_eq!(
            to_hex(ciphertext),
            "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d6\
             3dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b36\
             92ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc\
             3ff4def08e4b7a9de576d26586cec64b6116"
        );
        assert_eq!(to_hex(tag), "1ae10b594f09e26a7e902ecbd0600691");
        assert_eq!(
            open(&key, &nonce, &aad, &sealed).as_deref(),
            Some(&plaintext[..])
        );
    }

    #[test]
    fn authentication_rejects_modified_ciphertext_and_aad() {
        let key = [7u8; 32];
        let nonce = [9u8; 12];
        let mut sealed = seal(&key, &nonce, b"aad", b"payload bytes");
        sealed[0] ^= 1;
        assert!(open(&key, &nonce, b"aad", &sealed).is_none());

        let sealed = seal(&key, &nonce, b"aad", b"payload bytes");
        assert!(open(&key, &nonce, b"other aad", &sealed).is_none());
    }

    #[test]
    fn empty_and_multiblock_plaintexts_roundtrip() {
        let key = [0xa5; 32];
        let nonce = [3u8; 12];
        for plaintext in [Vec::new(), vec![0x5a; 257]] {
            let sealed = seal(&key, &nonce, b"", &plaintext);
            assert_eq!(open(&key, &nonce, b"", &sealed).unwrap(), plaintext);
        }
    }
}
