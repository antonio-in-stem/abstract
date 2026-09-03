//! Zero-dependency cryptography for Abstract bundles.
//!
//! Implements SHA-256 (FIPS 180-4) and the ChaCha20-Poly1305 AEAD
//! (RFC 8439). Both are verified against the official test vectors in the
//! unit tests below. ChaCha20-Poly1305 was chosen over AES because it is
//! straightforward to implement without lookup tables and the Java runtime
//! ships a matching zero-dependency implementation, so bundles produced by
//! the Rust CLI open on any JVM from Java 8 upward.

/// Computes the SHA-256 digest of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in message.chunks_exact(64) {
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

fn chacha20_quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] = (state[d] ^ state[a]).rotate_left(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_left(12);
    state[a] = state[a].wrapping_add(state[b]);
    state[d] = (state[d] ^ state[a]).rotate_left(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_left(7);
}

/// Produces one 64-byte ChaCha20 keystream block.
fn chacha20_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut state = [0u32; 16];
    state[0] = 0x61707865;
    state[1] = 0x3320646e;
    state[2] = 0x79622d32;
    state[3] = 0x6b206574;
    for index in 0..8 {
        state[4 + index] = u32::from_le_bytes([
            key[index * 4],
            key[index * 4 + 1],
            key[index * 4 + 2],
            key[index * 4 + 3],
        ]);
    }
    state[12] = counter;
    for index in 0..3 {
        state[13 + index] = u32::from_le_bytes([
            nonce[index * 4],
            nonce[index * 4 + 1],
            nonce[index * 4 + 2],
            nonce[index * 4 + 3],
        ]);
    }

    let mut working = state;
    for _ in 0..10 {
        chacha20_quarter_round(&mut working, 0, 4, 8, 12);
        chacha20_quarter_round(&mut working, 1, 5, 9, 13);
        chacha20_quarter_round(&mut working, 2, 6, 10, 14);
        chacha20_quarter_round(&mut working, 3, 7, 11, 15);
        chacha20_quarter_round(&mut working, 0, 5, 10, 15);
        chacha20_quarter_round(&mut working, 1, 6, 11, 12);
        chacha20_quarter_round(&mut working, 2, 7, 8, 13);
        chacha20_quarter_round(&mut working, 3, 4, 9, 14);
    }
    let mut output = [0u8; 64];
    for index in 0..16 {
        let word = working[index].wrapping_add(state[index]);
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    output
}

/// XORs `data` with the ChaCha20 keystream starting at block `counter`.
fn chacha20_xor(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &mut [u8]) {
    for (block_index, chunk) in data.chunks_mut(64).enumerate() {
        let block = chacha20_block(key, counter.wrapping_add(block_index as u32), nonce);
        for (byte, keystream) in chunk.iter_mut().zip(block.iter()) {
            *byte ^= keystream;
        }
    }
}

/// Computes a Poly1305 tag (RFC 8439, 26-bit limb implementation).
fn poly1305(key: &[u8; 32], message: &[u8]) -> [u8; 16] {
    let r0 = (u32::from_le_bytes([key[0], key[1], key[2], key[3]])) & 0x3ffffff;
    let r1 = (u32::from_le_bytes([key[3], key[4], key[5], key[6]]) >> 2) & 0x3ffff03;
    let r2 = (u32::from_le_bytes([key[6], key[7], key[8], key[9]]) >> 4) & 0x3ffc0ff;
    let r3 = (u32::from_le_bytes([key[9], key[10], key[11], key[12]]) >> 6) & 0x3f03fff;
    let r4 = (u32::from_le_bytes([key[12], key[13], key[14], key[15]]) >> 8) & 0x00fffff;

    let s1 = r1 * 5;
    let s2 = r2 * 5;
    let s3 = r3 * 5;
    let s4 = r4 * 5;

    let mut h0 = 0u32;
    let mut h1 = 0u32;
    let mut h2 = 0u32;
    let mut h3 = 0u32;
    let mut h4 = 0u32;

    let mut chunks = message.chunks_exact(16);
    let mut process = |block: &[u8], hibit: u32| {
        h0 = h0.wrapping_add(u32::from_le_bytes([block[0], block[1], block[2], block[3]]) & 0x3ffffff);
        h1 = h1.wrapping_add(
            (u32::from_le_bytes([block[3], block[4], block[5], block[6]]) >> 2) & 0x3ffffff,
        );
        h2 = h2.wrapping_add(
            (u32::from_le_bytes([block[6], block[7], block[8], block[9]]) >> 4) & 0x3ffffff,
        );
        h3 = h3.wrapping_add(
            (u32::from_le_bytes([block[9], block[10], block[11], block[12]]) >> 6) & 0x3ffffff,
        );
        h4 = h4.wrapping_add(
            (u32::from_le_bytes([block[12], block[13], block[14], block[15]]) >> 8) | hibit,
        );

        let d0 = (h0 as u64) * (r0 as u64)
            + (h1 as u64) * (s4 as u64)
            + (h2 as u64) * (s3 as u64)
            + (h3 as u64) * (s2 as u64)
            + (h4 as u64) * (s1 as u64);
        let mut d1 = (h0 as u64) * (r1 as u64)
            + (h1 as u64) * (r0 as u64)
            + (h2 as u64) * (s4 as u64)
            + (h3 as u64) * (s3 as u64)
            + (h4 as u64) * (s2 as u64);
        let mut d2 = (h0 as u64) * (r2 as u64)
            + (h1 as u64) * (r1 as u64)
            + (h2 as u64) * (r0 as u64)
            + (h3 as u64) * (s4 as u64)
            + (h4 as u64) * (s3 as u64);
        let mut d3 = (h0 as u64) * (r3 as u64)
            + (h1 as u64) * (r2 as u64)
            + (h2 as u64) * (r1 as u64)
            + (h3 as u64) * (r0 as u64)
            + (h4 as u64) * (s4 as u64);
        let mut d4 = (h0 as u64) * (r4 as u64)
            + (h1 as u64) * (r3 as u64)
            + (h2 as u64) * (r2 as u64)
            + (h3 as u64) * (r1 as u64)
            + (h4 as u64) * (r0 as u64);

        let mut carry = (d0 >> 26) as u64;
        h0 = (d0 & 0x3ffffff) as u32;
        d1 += carry;
        carry = d1 >> 26;
        h1 = (d1 & 0x3ffffff) as u32;
        d2 += carry;
        carry = d2 >> 26;
        h2 = (d2 & 0x3ffffff) as u32;
        d3 += carry;
        carry = d3 >> 26;
        h3 = (d3 & 0x3ffffff) as u32;
        d4 += carry;
        carry = d4 >> 26;
        h4 = (d4 & 0x3ffffff) as u32;
        h0 = h0.wrapping_add((carry as u32) * 5);
        let carry = h0 >> 26;
        h0 &= 0x3ffffff;
        h1 = h1.wrapping_add(carry);
    };

    for block in chunks.by_ref() {
        process(block, 1 << 24);
    }
    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        let mut block = [0u8; 16];
        block[..remainder.len()].copy_from_slice(remainder);
        block[remainder.len()] = 0x01;
        process(&block, 0);
    }

    // Full carry propagation.
    let mut carry = h1 >> 26;
    h1 &= 0x3ffffff;
    h2 = h2.wrapping_add(carry);
    carry = h2 >> 26;
    h2 &= 0x3ffffff;
    h3 = h3.wrapping_add(carry);
    carry = h3 >> 26;
    h3 &= 0x3ffffff;
    h4 = h4.wrapping_add(carry);
    carry = h4 >> 26;
    h4 &= 0x3ffffff;
    h0 = h0.wrapping_add(carry * 5);
    carry = h0 >> 26;
    h0 &= 0x3ffffff;
    h1 = h1.wrapping_add(carry);

    // Compute h + -p and select it if h >= p.
    let mut g0 = h0.wrapping_add(5);
    carry = g0 >> 26;
    g0 &= 0x3ffffff;
    let mut g1 = h1.wrapping_add(carry);
    carry = g1 >> 26;
    g1 &= 0x3ffffff;
    let mut g2 = h2.wrapping_add(carry);
    carry = g2 >> 26;
    g2 &= 0x3ffffff;
    let mut g3 = h3.wrapping_add(carry);
    carry = g3 >> 26;
    g3 &= 0x3ffffff;
    let g4 = h4.wrapping_add(carry).wrapping_sub(1 << 26);

    let mask = (g4 >> 31).wrapping_sub(1);
    h0 = (h0 & !mask) | (g0 & mask);
    h1 = (h1 & !mask) | (g1 & mask);
    h2 = (h2 & !mask) | (g2 & mask);
    h3 = (h3 & !mask) | (g3 & mask);
    h4 = (h4 & !mask) | (g4 & mask);

    // Serialize back to 128 bits and add the pad.
    let f0 = ((h0) | (h1 << 26)) as u64;
    let f1 = ((h1 >> 6) | (h2 << 20)) as u64;
    let f2 = ((h2 >> 12) | (h3 << 14)) as u64;
    let f3 = ((h3 >> 18) | (h4 << 8)) as u64;

    let pad0 = u32::from_le_bytes([key[16], key[17], key[18], key[19]]) as u64;
    let pad1 = u32::from_le_bytes([key[20], key[21], key[22], key[23]]) as u64;
    let pad2 = u32::from_le_bytes([key[24], key[25], key[26], key[27]]) as u64;
    let pad3 = u32::from_le_bytes([key[28], key[29], key[30], key[31]]) as u64;

    let mut acc = (f0 & 0xffffffff) + pad0;
    let out0 = acc as u32;
    acc = (acc >> 32) + (f1 & 0xffffffff) + pad1;
    let out1 = acc as u32;
    acc = (acc >> 32) + (f2 & 0xffffffff) + pad2;
    let out2 = acc as u32;
    acc = (acc >> 32) + (f3 & 0xffffffff) + pad3;
    let out3 = acc as u32;

    let mut tag = [0u8; 16];
    tag[0..4].copy_from_slice(&out0.to_le_bytes());
    tag[4..8].copy_from_slice(&out1.to_le_bytes());
    tag[8..12].copy_from_slice(&out2.to_le_bytes());
    tag[12..16].copy_from_slice(&out3.to_le_bytes());
    tag
}

fn poly1305_aead_mac(
    otk: &[u8; 32],
    aad: &[u8],
    ciphertext: &[u8],
) -> [u8; 16] {
    let mut mac_data = Vec::with_capacity(aad.len() + ciphertext.len() + 32);
    mac_data.extend_from_slice(aad);
    while mac_data.len() % 16 != 0 {
        mac_data.push(0);
    }
    mac_data.extend_from_slice(ciphertext);
    while mac_data.len() % 16 != 0 {
        mac_data.push(0);
    }
    mac_data.extend_from_slice(&(aad.len() as u64).to_le_bytes());
    mac_data.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());
    poly1305(otk, &mac_data)
}

fn poly1305_key(key: &[u8; 32], nonce: &[u8; 12]) -> [u8; 32] {
    let block = chacha20_block(key, 0, nonce);
    let mut otk = [0u8; 32];
    otk.copy_from_slice(&block[..32]);
    otk
}

/// Encrypts `plaintext` and returns `ciphertext || 16-byte tag`.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let mut output = plaintext.to_vec();
    chacha20_xor(key, 1, nonce, &mut output);
    let tag = poly1305_aead_mac(&poly1305_key(key, nonce), aad, &output);
    output.extend_from_slice(&tag);
    output
}

/// Verifies and decrypts `ciphertext || tag`. Returns `None` when the tag
/// does not authenticate (wrong key, wrong nonce, or tampered data).
pub fn open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 16 {
        return None;
    }
    let (ciphertext, tag) = data.split_at(data.len() - 16);
    let expected = poly1305_aead_mac(&poly1305_key(key, nonce), aad, ciphertext);
    let mut difference = 0u8;
    for (left, right) in expected.iter().zip(tag.iter()) {
        difference |= left ^ right;
    }
    if difference != 0 {
        return None;
    }
    let mut output = ciphertext.to_vec();
    chacha20_xor(key, 1, nonce, &mut output);
    Some(output)
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
    fn chacha20_block_matches_rfc_8439_vector() {
        // RFC 8439 section 2.3.2.
        let key: [u8; 32] = hex(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .try_into()
        .unwrap();
        let nonce: [u8; 12] = hex("000000090000004a00000000").try_into().unwrap();
        let block = chacha20_block(&key, 1, &nonce);
        assert_eq!(
            to_hex(&block),
            "10f1e7e4d13b5915500fdd1fa32071c4c7d1f4c733c068030422aa9ac3d46c4e\
             d2826446079faa0914c2d705d98b02a2b5129cd1de164eb9cbd083e8a2503c4e"
        );
    }

    #[test]
    fn poly1305_matches_rfc_8439_vector() {
        // RFC 8439 section 2.5.2.
        let key: [u8; 32] = hex(
            "85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b",
        )
        .try_into()
        .unwrap();
        let tag = poly1305(&key, b"Cryptographic Forum Research Group");
        assert_eq!(to_hex(&tag), "a8061dc1305136c6c22b8baf0c0127a9");
    }

    #[test]
    fn aead_matches_rfc_8439_vector() {
        // RFC 8439 section 2.8.2.
        let key: [u8; 32] = hex(
            "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f",
        )
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

        let opened = open(&key, &nonce, &aad, &sealed).expect("tag should verify");
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn open_rejects_tampered_data() {
        let key = [7u8; 32];
        let nonce = [9u8; 12];
        let mut sealed = seal(&key, &nonce, b"aad", b"payload bytes");
        sealed[0] ^= 0x01;
        assert!(open(&key, &nonce, b"aad", &sealed).is_none());
        let sealed = seal(&key, &nonce, b"aad", b"payload bytes");
        assert!(open(&key, &nonce, b"other aad", &sealed).is_none());
    }

    #[test]
    fn roundtrips_multi_block_payloads() {
        let key = [42u8; 32];
        let nonce = [3u8; 12];
        let plaintext: Vec<u8> = (0..1000u32).map(|value| (value % 251) as u8).collect();
        let sealed = seal(&key, &nonce, b"", &plaintext);
        assert_eq!(open(&key, &nonce, b"", &sealed).unwrap(), plaintext);
    }
}
