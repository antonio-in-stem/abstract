package com.abstractlang.runtime;

import java.util.Arrays;
import org.bouncycastle.crypto.InvalidCipherTextException;
import org.bouncycastle.crypto.params.AEADParameters;
import org.bouncycastle.crypto.params.KeyParameter;

/**
 * Small ABX1 adapter over Bouncy Castle's ChaCha20-Poly1305 implementation.
 * The wire result is ciphertext followed by the 16-byte authentication tag,
 * as required by RFC 8439 and the existing Abstract bundle format.
 */
final class ChaCha20Poly1305 {

    private static final int KEY_LENGTH = 32;
    private static final int NONCE_LENGTH = 12;
    private static final int TAG_BITS = 128;
    private static final int TAG_LENGTH = 16;

    private ChaCha20Poly1305() {
    }

    /** Returns plaintext when authentication succeeds, or {@code null}. */
    static byte[] open(byte[] key, byte[] nonce, byte[] aad, byte[] data) {
        if (!validInputs(key, nonce, aad, data) || data.length < TAG_LENGTH) {
            return null;
        }
        return process(false, key, nonce, aad, data);
    }

    /** Returns ciphertext followed by its 16-byte authentication tag. */
    static byte[] seal(byte[] key, byte[] nonce, byte[] aad, byte[] plaintext) {
        if (!validInputs(key, nonce, aad, plaintext)) {
            throw new IllegalArgumentException(
                    "ChaCha20-Poly1305 requires a 32-byte key and 12-byte nonce");
        }
        byte[] sealed = process(true, key, nonce, aad, plaintext);
        if (sealed == null) {
            throw new IllegalStateException("ChaCha20-Poly1305 encryption failed");
        }
        return sealed;
    }

    private static byte[] process(
            boolean encrypt, byte[] key, byte[] nonce, byte[] aad, byte[] input) {
        org.bouncycastle.crypto.modes.ChaCha20Poly1305 cipher =
                new org.bouncycastle.crypto.modes.ChaCha20Poly1305();
        try {
            cipher.init(encrypt,
                    new AEADParameters(new KeyParameter(key), TAG_BITS, nonce, aad));
            byte[] output = new byte[cipher.getOutputSize(input.length)];
            int written = cipher.processBytes(input, 0, input.length, output, 0);
            written += cipher.doFinal(output, written);
            return written == output.length ? output : Arrays.copyOf(output, written);
        } catch (InvalidCipherTextException | RuntimeException exception) {
            return null;
        }
    }

    private static boolean validInputs(byte[] key, byte[] nonce, byte[] aad, byte[] input) {
        return key != null && key.length == KEY_LENGTH
                && nonce != null && nonce.length == NONCE_LENGTH
                && aad != null && input != null;
    }
}
