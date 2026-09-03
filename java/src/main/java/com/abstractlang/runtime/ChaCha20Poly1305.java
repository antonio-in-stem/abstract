package com.abstractlang.runtime;

import java.security.MessageDigest;

/**
 * Zero-dependency ChaCha20-Poly1305 AEAD (RFC 8439) for opening Abstract
 * bundles. Runs on Java 8 and newer, so it works in every Minecraft server
 * or standalone JVM without extra libraries.
 *
 * <p>The implementation mirrors the Rust encoder shipped with the Abstract
 * CLI and is verified against the official RFC 8439 test vectors (see
 * {@code Selftest} and the JUnit tests).
 */
final class ChaCha20Poly1305 {

    private ChaCha20Poly1305() {
    }

    /**
     * Verifies and decrypts {@code ciphertext || 16-byte tag}. Returns the
     * plaintext, or {@code null} when the tag does not authenticate (wrong
     * key, wrong nonce, or tampered data).
     */
    static byte[] open(byte[] key, byte[] nonce, byte[] aad, byte[] data) {
        checkKeyAndNonce(key, nonce);
        if (data.length < 16) {
            return null;
        }
        int ciphertextLength = data.length - 16;
        byte[] ciphertext = new byte[ciphertextLength];
        System.arraycopy(data, 0, ciphertext, 0, ciphertextLength);
        byte[] tag = new byte[16];
        System.arraycopy(data, ciphertextLength, tag, 0, 16);

        byte[] oneTimeKey = poly1305Key(key, nonce);
        byte[] expected = aeadMac(oneTimeKey, aad, ciphertext);
        if (!MessageDigest.isEqual(expected, tag)) {
            return null;
        }
        byte[] plaintext = ciphertext.clone();
        chacha20Xor(key, 1, nonce, plaintext);
        return plaintext;
    }

    /** Encrypts and authenticates; used by tests to prove interop. */
    static byte[] seal(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext) {
        checkKeyAndNonce(key, nonce);
        byte[] ciphertext = plaintext.clone();
        chacha20Xor(key, 1, nonce, ciphertext);
        byte[] tag = aeadMac(poly1305Key(key, nonce), aad, ciphertext);
        byte[] output = new byte[ciphertext.length + 16];
        System.arraycopy(ciphertext, 0, output, 0, ciphertext.length);
        System.arraycopy(tag, 0, output, ciphertext.length, 16);
        return output;
    }

    private static void checkKeyAndNonce(byte[] key, byte[] nonce) {
        if (key == null || key.length != 32) {
            throw new IllegalArgumentException("key must be exactly 32 bytes");
        }
        if (nonce == null || nonce.length != 12) {
            throw new IllegalArgumentException("nonce must be exactly 12 bytes");
        }
    }

    // ------------------------------------------------------------------
    // ChaCha20
    // ------------------------------------------------------------------

    static byte[] chacha20Block(byte[] key, int counter, byte[] nonce) {
        int[] state = new int[16];
        state[0] = 0x61707865;
        state[1] = 0x3320646e;
        state[2] = 0x79622d32;
        state[3] = 0x6b206574;
        for (int i = 0; i < 8; i++) {
            state[4 + i] = load32(key, i * 4);
        }
        state[12] = counter;
        for (int i = 0; i < 3; i++) {
            state[13 + i] = load32(nonce, i * 4);
        }

        int[] working = state.clone();
        for (int round = 0; round < 10; round++) {
            quarterRound(working, 0, 4, 8, 12);
            quarterRound(working, 1, 5, 9, 13);
            quarterRound(working, 2, 6, 10, 14);
            quarterRound(working, 3, 7, 11, 15);
            quarterRound(working, 0, 5, 10, 15);
            quarterRound(working, 1, 6, 11, 12);
            quarterRound(working, 2, 7, 8, 13);
            quarterRound(working, 3, 4, 9, 14);
        }
        byte[] output = new byte[64];
        for (int i = 0; i < 16; i++) {
            store32(output, i * 4, working[i] + state[i]);
        }
        return output;
    }

    private static void quarterRound(int[] state, int a, int b, int c, int d) {
        state[a] += state[b];
        state[d] = Integer.rotateLeft(state[d] ^ state[a], 16);
        state[c] += state[d];
        state[b] = Integer.rotateLeft(state[b] ^ state[c], 12);
        state[a] += state[b];
        state[d] = Integer.rotateLeft(state[d] ^ state[a], 8);
        state[c] += state[d];
        state[b] = Integer.rotateLeft(state[b] ^ state[c], 7);
    }

    private static void chacha20Xor(byte[] key, int counter, byte[] nonce, byte[] data) {
        for (int offset = 0; offset < data.length; offset += 64) {
            byte[] block = chacha20Block(key, counter + offset / 64, nonce);
            int limit = Math.min(64, data.length - offset);
            for (int i = 0; i < limit; i++) {
                data[offset + i] ^= block[i];
            }
        }
    }

    private static byte[] poly1305Key(byte[] key, byte[] nonce) {
        byte[] block = chacha20Block(key, 0, nonce);
        byte[] oneTimeKey = new byte[32];
        System.arraycopy(block, 0, oneTimeKey, 0, 32);
        return oneTimeKey;
    }

    // ------------------------------------------------------------------
    // Poly1305 (26-bit limbs in longs, mirrors the Rust implementation)
    // ------------------------------------------------------------------

    static byte[] poly1305(byte[] key, byte[] message) {
        long r0 = load32Long(key, 0) & 0x3ffffffL;
        long r1 = (load32Long(key, 3) >> 2) & 0x3ffff03L;
        long r2 = (load32Long(key, 6) >> 4) & 0x3ffc0ffL;
        long r3 = (load32Long(key, 9) >> 6) & 0x3f03fffL;
        long r4 = (load32Long(key, 12) >> 8) & 0x00fffffL;

        long s1 = r1 * 5;
        long s2 = r2 * 5;
        long s3 = r3 * 5;
        long s4 = r4 * 5;

        long h0 = 0, h1 = 0, h2 = 0, h3 = 0, h4 = 0;

        int offset = 0;
        while (offset < message.length) {
            int chunk = Math.min(16, message.length - offset);
            byte[] block = new byte[16];
            System.arraycopy(message, offset, block, 0, chunk);
            long hibit;
            if (chunk == 16) {
                hibit = 1L << 24;
            } else {
                block[chunk] = 0x01;
                hibit = 0;
            }

            h0 += load32Long(block, 0) & 0x3ffffffL;
            h1 += (load32Long(block, 3) >> 2) & 0x3ffffffL;
            h2 += (load32Long(block, 6) >> 4) & 0x3ffffffL;
            h3 += (load32Long(block, 9) >> 6) & 0x3ffffffL;
            h4 += (load32Long(block, 12) >> 8) | hibit;

            long d0 = h0 * r0 + h1 * s4 + h2 * s3 + h3 * s2 + h4 * s1;
            long d1 = h0 * r1 + h1 * r0 + h2 * s4 + h3 * s3 + h4 * s2;
            long d2 = h0 * r2 + h1 * r1 + h2 * r0 + h3 * s4 + h4 * s3;
            long d3 = h0 * r3 + h1 * r2 + h2 * r1 + h3 * r0 + h4 * s4;
            long d4 = h0 * r4 + h1 * r3 + h2 * r2 + h3 * r1 + h4 * r0;

            long carry = d0 >>> 26;
            h0 = d0 & 0x3ffffffL;
            d1 += carry;
            carry = d1 >>> 26;
            h1 = d1 & 0x3ffffffL;
            d2 += carry;
            carry = d2 >>> 26;
            h2 = d2 & 0x3ffffffL;
            d3 += carry;
            carry = d3 >>> 26;
            h3 = d3 & 0x3ffffffL;
            d4 += carry;
            carry = d4 >>> 26;
            h4 = d4 & 0x3ffffffL;
            h0 += carry * 5;
            carry = h0 >>> 26;
            h0 &= 0x3ffffffL;
            h1 += carry;

            offset += chunk;
        }

        long carry = h1 >>> 26;
        h1 &= 0x3ffffffL;
        h2 += carry;
        carry = h2 >>> 26;
        h2 &= 0x3ffffffL;
        h3 += carry;
        carry = h3 >>> 26;
        h3 &= 0x3ffffffL;
        h4 += carry;
        carry = h4 >>> 26;
        h4 &= 0x3ffffffL;
        h0 += carry * 5;
        carry = h0 >>> 26;
        h0 &= 0x3ffffffL;
        h1 += carry;

        // Freeze: compute h - p and select it when there is no borrow.
        long g0 = h0 + 5;
        carry = g0 >>> 26;
        g0 &= 0x3ffffffL;
        long g1 = h1 + carry;
        carry = g1 >>> 26;
        g1 &= 0x3ffffffL;
        long g2 = h2 + carry;
        carry = g2 >>> 26;
        g2 &= 0x3ffffffL;
        long g3 = h3 + carry;
        carry = g3 >>> 26;
        g3 &= 0x3ffffffL;
        long g4 = h4 + carry - (1L << 26);

        if (g4 >= 0) {
            h0 = g0;
            h1 = g1;
            h2 = g2;
            h3 = g3;
            h4 = g4;
        }

        long f0 = (h0 | (h1 << 26)) & 0xffffffffL;
        long f1 = ((h1 >>> 6) | (h2 << 20)) & 0xffffffffL;
        long f2 = ((h2 >>> 12) | (h3 << 14)) & 0xffffffffL;
        long f3 = ((h3 >>> 18) | (h4 << 8)) & 0xffffffffL;

        long acc = f0 + load32Long(key, 16);
        byte[] tag = new byte[16];
        store32(tag, 0, (int) acc);
        acc = (acc >>> 32) + f1 + load32Long(key, 20);
        store32(tag, 4, (int) acc);
        acc = (acc >>> 32) + f2 + load32Long(key, 24);
        store32(tag, 8, (int) acc);
        acc = (acc >>> 32) + f3 + load32Long(key, 28);
        store32(tag, 12, (int) acc);
        return tag;
    }

    private static byte[] aeadMac(byte[] oneTimeKey, byte[] aad, byte[] ciphertext) {
        int aadPadding = (16 - aad.length % 16) % 16;
        int ciphertextPadding = (16 - ciphertext.length % 16) % 16;
        byte[] macData = new byte[aad.length + aadPadding + ciphertext.length + ciphertextPadding + 16];
        int cursor = 0;
        System.arraycopy(aad, 0, macData, cursor, aad.length);
        cursor += aad.length + aadPadding;
        System.arraycopy(ciphertext, 0, macData, cursor, ciphertext.length);
        cursor += ciphertext.length + ciphertextPadding;
        store64(macData, cursor, aad.length);
        store64(macData, cursor + 8, ciphertext.length);
        return poly1305(oneTimeKey, macData);
    }

    // ------------------------------------------------------------------
    // Byte helpers
    // ------------------------------------------------------------------

    private static int load32(byte[] source, int offset) {
        return (source[offset] & 0xff)
                | (source[offset + 1] & 0xff) << 8
                | (source[offset + 2] & 0xff) << 16
                | (source[offset + 3] & 0xff) << 24;
    }

    private static long load32Long(byte[] source, int offset) {
        return load32(source, offset) & 0xffffffffL;
    }

    private static void store32(byte[] target, int offset, int value) {
        target[offset] = (byte) value;
        target[offset + 1] = (byte) (value >>> 8);
        target[offset + 2] = (byte) (value >>> 16);
        target[offset + 3] = (byte) (value >>> 24);
    }

    private static void store64(byte[] target, int offset, long value) {
        for (int i = 0; i < 8; i++) {
            target[offset + i] = (byte) (value >>> (8 * i));
        }
    }
}
