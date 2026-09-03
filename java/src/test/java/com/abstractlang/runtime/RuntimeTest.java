package com.abstractlang.runtime;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.Test;

/** JUnit mirror of the dependency-free {@code Selftest} checks. */
class RuntimeTest {

    private static byte[] hex(String text) {
        byte[] output = new byte[text.length() / 2];
        for (int i = 0; i < output.length; i++) {
            output[i] = (byte) Integer.parseInt(text.substring(i * 2, i * 2 + 2), 16);
        }
        return output;
    }

    private static String toHex(byte[] bytes) {
        StringBuilder builder = new StringBuilder();
        for (byte value : bytes) {
            builder.append(String.format("%02x", value));
        }
        return builder.toString();
    }

    @Test
    void poly1305MatchesRfc8439Vector() {
        byte[] key = hex("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b");
        byte[] tag = ChaCha20Poly1305.poly1305(
                key, "Cryptographic Forum Research Group".getBytes(StandardCharsets.US_ASCII));
        assertEquals("a8061dc1305136c6c22b8baf0c0127a9", toHex(tag));
    }

    @Test
    void aeadMatchesRfc8439VectorAndRejectsTampering() {
        byte[] key = hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
        byte[] nonce = hex("070000004041424344454647");
        byte[] aad = hex("50515253c0c1c2c3c4c5c6c7");
        byte[] plaintext = ("Ladies and Gentlemen of the class of '99: If I could offer you "
                + "only one tip for the future, sunscreen would be it.")
                .getBytes(StandardCharsets.US_ASCII);

        byte[] sealed = ChaCha20Poly1305.seal(key, nonce, aad, plaintext);
        assertTrue(toHex(sealed).endsWith("1ae10b594f09e26a7e902ecbd0600691"));

        byte[] opened = ChaCha20Poly1305.open(key, nonce, aad, sealed);
        assertEquals(new String(plaintext, StandardCharsets.US_ASCII),
                new String(opened, StandardCharsets.US_ASCII));

        sealed[0] ^= 1;
        assertNull(ChaCha20Poly1305.open(key, nonce, aad, sealed));
    }

    @Test
    void keySharesRecombine() {
        byte[] key = AbstractKeys.fromPassphrase("sunny meadows");
        byte[][] shares = AbstractKeys.split(key, 4);
        assertEquals(toHex(key), toHex(AbstractKeys.combine(shares)));
    }

    @Test
    void jsonDocumentsExposeTypedValues() {
        AbstractData data = AbstractData.fromJson(
                "{\"data\": [{\"template\": \"Item\", \"id\": \"coin\", \"price\": 9.5,"
                        + " \"tradable\": true, \"stats\": {\"power\": 12}}]}");
        AbstractObject coin = data.require("coin");
        assertEquals(9.5, coin.getFloat("price"));
        assertTrue(coin.getBool("tradable"));
        assertEquals(12, coin.getInt("stats.power"));
        assertEquals(1, data.byTemplate("Item").size());
    }

    @Test
    void plainBundleWithKeyIsRefused() {
        // Header: magic, flags=0 (plain), format=1, zero nonce, length=2, "{}"
        byte[] bundle = new byte[24];
        bundle[0] = 'A';
        bundle[1] = 'B';
        bundle[2] = 'X';
        bundle[3] = '1';
        bundle[5] = 1;
        bundle[18] = 2;
        bundle[22] = '{';
        bundle[23] = '}';

        assertEquals("{}", new String(
                AbstractBundle.openPayload(bundle, null), StandardCharsets.UTF_8));
        assertThrows(AbstractDataException.class,
                () -> AbstractBundle.openPayload(bundle, new byte[32]));
    }
}
