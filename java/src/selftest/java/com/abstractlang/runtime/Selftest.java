package com.abstractlang.runtime;

import java.io.FileInputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

/**
 * Dependency-free verification runner. Checks the RFC 8439 vectors and then,
 * when given a bundle path and key, performs a full cross-language load of a
 * bundle produced by the Rust CLI.
 *
 * <pre>
 * javac -d out $(find src -name '*.java')
 * java -cp out com.abstractlang.runtime.Selftest [bundle.abx passphrase]
 * </pre>
 */
public final class Selftest {

    public static void main(String[] args) throws Exception {
        chachaBlockVector();
        poly1305Vector();
        aeadVector();
        keyHelpers();
        jsonParser();
        System.out.println("selftest: all RFC 8439 vectors and unit checks passed");

        if (args.length >= 2) {
            loadBundle(args[0], args[1]);
        }
    }

    private static void chachaBlockVector() {
        byte[] key = hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        byte[] nonce = hex("000000090000004a00000000");
        byte[] block = ChaCha20Poly1305.chacha20Block(key, 1, nonce);
        expectEquals(
                "chacha20 block",
                "10f1e7e4d13b5915500fdd1fa32071c4c7d1f4c733c068030422aa9ac3d46c4e"
                        + "d2826446079faa0914c2d705d98b02a2b5129cd1de164eb9cbd083e8a2503c4e",
                toHex(block));
    }

    private static void poly1305Vector() {
        byte[] key = hex("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b");
        byte[] tag = ChaCha20Poly1305.poly1305(
                key, "Cryptographic Forum Research Group".getBytes(StandardCharsets.US_ASCII));
        expectEquals("poly1305 tag", "a8061dc1305136c6c22b8baf0c0127a9", toHex(tag));
    }

    private static void aeadVector() {
        byte[] key = hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
        byte[] nonce = hex("070000004041424344454647");
        byte[] aad = hex("50515253c0c1c2c3c4c5c6c7");
        byte[] plaintext = ("Ladies and Gentlemen of the class of '99: If I could offer you "
                + "only one tip for the future, sunscreen would be it.")
                .getBytes(StandardCharsets.US_ASCII);

        byte[] sealed = ChaCha20Poly1305.seal(key, nonce, aad, plaintext);
        String tagHex = toHex(sealed).substring((sealed.length - 16) * 2);
        expectEquals("aead tag", "1ae10b594f09e26a7e902ecbd0600691", tagHex);

        byte[] opened = ChaCha20Poly1305.open(key, nonce, aad, sealed);
        if (opened == null) {
            throw new AssertionError("aead open failed on valid data");
        }
        expectEquals("aead roundtrip", new String(plaintext, StandardCharsets.US_ASCII),
                new String(opened, StandardCharsets.US_ASCII));

        sealed[3] ^= 0x20;
        if (ChaCha20Poly1305.open(key, nonce, aad, sealed) != null) {
            throw new AssertionError("aead open accepted tampered data");
        }
    }

    private static void keyHelpers() {
        byte[] key = AbstractKeys.fromPassphrase("sunny meadows");
        byte[][] shares = AbstractKeys.split(key, 3);
        byte[] combined = AbstractKeys.combine(shares);
        expectEquals("key split/combine", toHex(key), toHex(combined));
    }

    private static void jsonParser() {
        AbstractData data = AbstractData.fromJson(
                "{\"data\": [{\"template\": \"Item\", \"id\": \"coin\", \"price\": 9.5,"
                        + " \"tradable\": true, \"tags\": [\"a\", \"b\"],"
                        + " \"stats\": {\"power\": 12}}]}");
        AbstractObject coin = data.require("coin");
        expectEquals("json string", "Item", coin.template());
        if (coin.getFloat("price") != 9.5) {
            throw new AssertionError("float value lost precision");
        }
        if (!coin.getBool("tradable")) {
            throw new AssertionError("bool value lost");
        }
        if (coin.getInt("stats.power") != 12) {
            throw new AssertionError("dotted path failed");
        }
        if (coin.getStrings("tags").size() != 2) {
            throw new AssertionError("list access failed");
        }
    }

    private static void loadBundle(String path, String passphrase) throws Exception {
        byte[] key = AbstractKeys.fromPassphrase(passphrase);
        InputStream stream = new FileInputStream(path);
        AbstractData data;
        try {
            data = AbstractBundle.load(stream, key);
        } finally {
            stream.close();
        }
        System.out.println("selftest: opened bundle with " + data.size() + " instance(s)");
        for (AbstractObject object : data) {
            System.out.println("  - " + object.template() + " :: " + object.id());
        }
        if (data.size() == 0) {
            throw new AssertionError("bundle contained no instances");
        }
    }

    // ------------------------------------------------------------------

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

    private static void expectEquals(String label, String expected, String actual) {
        if (!expected.equals(actual)) {
            throw new AssertionError(label + " mismatch\n  expected: " + expected
                    + "\n  actual:   " + actual);
        }
    }
}
