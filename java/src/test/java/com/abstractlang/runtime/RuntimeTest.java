package com.abstractlang.runtime;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
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

    // ------------------------------------------------------------------
    // Key material (SPEC Appendix D.2 items 9 and 10)
    // ------------------------------------------------------------------

    @Test
    void badHexKeysAreThisRuntimesOwnException() {
        // Never NullPointerException, never NumberFormatException.
        assertThrows(AbstractDataException.class, () -> AbstractKeys.fromHex(null));
        assertThrows(AbstractDataException.class, () -> AbstractKeys.fromHex("abcd"));
        assertThrows(AbstractDataException.class, () -> AbstractKeys.fromHex(
                "zz0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"));
        assertThrows(AbstractDataException.class, () -> AbstractKeys.fromHex(
                "00 0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"));
        assertThrows(AbstractDataException.class, () -> AbstractKeys.fromPassphrase(null));
        assertThrows(AbstractDataException.class, () -> AbstractKeys.fromKeyMaterial(null));
    }

    @Test
    void onlyTheHexPrefixSelectsRawKeyBytes() {
        String digits = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        assertEquals(digits, toHex(AbstractKeys.fromKeyMaterial("hex:" + digits)));
        // A bare 64-character hexadecimal string is a passphrase, and is
        // hashed, exactly as the compiler treats it.
        assertEquals(toHex(AbstractKeys.fromPassphrase(digits)),
                toHex(AbstractKeys.fromKeyMaterial(digits)));
    }

    // ------------------------------------------------------------------
    // The JSON reader (SPEC Appendix D.2 items 6, 7 and 8)
    // ------------------------------------------------------------------

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
    void deeplyNestedDocumentsAreRefusedAndNeverOverflowTheStack() {
        StringBuilder deep = new StringBuilder("{\"data\": ");
        int levels = Json.MAX_DEPTH + 40;
        for (int i = 0; i < levels; i++) {
            deep.append('[');
        }
        for (int i = 0; i < levels; i++) {
            deep.append(']');
        }
        deep.append('}');
        AbstractDataException error = assertThrows(AbstractDataException.class,
                () -> AbstractData.fromJson(deep.toString()));
        assertTrue(error.getMessage().contains("nests deeper than"), error.getMessage());
    }

    @Test
    void strictNumbersAreEnforced() {
        String[] rejected = {"+1", "01", "007", "-01", "1.", ".5", "1e", "1e+", "-", "1.2.3",
            "0x10", "Infinity", "NaN"};
        for (String number : rejected) {
            String document = "{\"data\": [], \"n\": " + number + "}";
            assertThrows(AbstractDataException.class, () -> AbstractData.fromJson(document),
                    "should reject " + number);
        }

        String[] accepted = {"0", "-0", "1", "-1", "0.5", "-0.5", "1e3", "1E-3", "1.5e+3",
            "9223372036854775807"};
        for (String number : accepted) {
            String document = "{\"data\": [], \"n\": " + number + "}";
            assertNotNull(AbstractData.fromJson(document), "should accept " + number);
        }
    }

    @Test
    void duplicateKeysAreRefused() {
        AbstractDataException error = assertThrows(AbstractDataException.class,
                () -> AbstractData.fromJson(
                        "{\"data\": [{\"id\": \"a\", \"id\": \"b\"}]}"));
        assertTrue(error.getMessage().contains("duplicate object key"), error.getMessage());
    }

    @Test
    void loneSurrogatesAndMalformedEscapesAreRefused() {
        // A high surrogate with no partner, a low surrogate on its own, and a
        // \\u escape whose digits are not hexadecimal.
        String[] rejected = {
            "{\"data\": [], \"s\": \"\\ud83d\"}",
            "{\"data\": [], \"s\": \"\\udc00\"}",
            "{\"data\": [], \"s\": \"\\ud83dx\"}",
            "{\"data\": [], \"s\": \"\\ud83d\\u0041\"}",
            "{\"data\": [], \"s\": \"\\uzzzz\"}",
            "{\"data\": [], \"s\": \"\\u12\"}",
            "{\"data\": [], \"s\": \"\\q\"}",
        };
        for (String document : rejected) {
            assertThrows(AbstractDataException.class, () -> AbstractData.fromJson(document),
                    "should reject " + document);
        }
        // A well-formed pair still reads back as one code point.
        AbstractData data = AbstractData.fromJson(
                "{\"data\": [{\"template\": \"T\", \"id\": \"a\", \"s\": \"\\ud83d\\ude00\"}]}");
        assertEquals(1, data.require("a").getString("s").codePointCount(0, 2));
    }

    @Test
    void unescapedControlCharactersAreRefused() {
        assertThrows(AbstractDataException.class,
                () -> AbstractData.fromJson("{\"data\": [], \"s\": \"a\nb\"}"));
    }

    // ------------------------------------------------------------------
    // The version axis (SPEC §7.5 step 3, Appendix D.2 item 11)
    // ------------------------------------------------------------------

    /**
     * The compiled document of {@code docs/examples/overlays-multi-match}, whose
     * README derives it from the sources by hand. Version 1 and version 2 are
     * each covered by two overlays, and {@code lantern} appears only in an
     * overlay, so a runtime that assumes one overlay per version or that every
     * overlay id is already in the base gets both versions wrong.
     */
    private static final String OVERLAY_DOCUMENT = "{"
            + "\"abstract\": {\"format\": 1, \"compiler\": \"1.0.0\","
            + " \"versions\": {\"min\": 1, \"max\": 3}},"
            + "\"data\": ["
            + "  {\"template\": \"Item\", \"id\": \"beacon\", \"name\": \"Beacon\","
            + "   \"icon\": \"textures/item.png\", \"glow\": true},"
            + "  {\"template\": \"Item\", \"id\": \"ember\", \"name\": \"Ember\","
            + "   \"icon\": \"textures/item.png\", \"glow\": false},"
            + "  {\"template\": \"Item\", \"id\": \"torch\", \"name\": \"Torch\","
            + "   \"icon\": \"textures/item.png\", \"glow\": false}],"
            + "\"overlays\": ["
            + "  {\"versions\": {\"min\": 1, \"max\": 1}, \"data\": ["
            + "     {\"template\": \"Item\", \"id\": \"ember\", \"name\": \"Ember\","
            + "      \"icon\": \"textures/item.png\", \"legacy_tint\": 40, \"tier\": 3}],"
            + "   \"removed\": []},"
            + "  {\"versions\": {\"min\": 1, \"max\": 2}, \"data\": ["
            + "     {\"template\": \"Item\", \"id\": \"lantern\", \"name\": \"Lantern\","
            + "      \"icon\": \"textures/item.png\", \"legacy_tint\": 10},"
            + "     {\"template\": \"Item\", \"id\": \"torch\", \"name\": \"Torch\","
            + "      \"icon\": \"textures/item.png\", \"legacy_tint\": 200}],"
            + "   \"removed\": [\"beacon\"]},"
            + "  {\"versions\": {\"min\": 2, \"max\": 2}, \"data\": ["
            + "     {\"template\": \"Item\", \"id\": \"ember\", \"name\": \"Ember\","
            + "      \"icon\": \"textures/item.png\", \"legacy_tint\": 40}],"
            + "   \"removed\": []}]}";

    private static List<String> idsOf(AbstractData data) {
        List<String> ids = new ArrayList<String>();
        for (AbstractObject object : data) {
            ids.add(object.id());
        }
        return ids;
    }

    @Test
    void theBaseIsTheMaximumVersion() {
        AbstractData data = AbstractData.fromJson(OVERLAY_DOCUMENT);
        assertEquals(1, data.minVersion());
        assertEquals(3, data.maxVersion());
        assertEquals(3, data.overlayCount());
        assertEquals(java.util.Arrays.asList("beacon", "ember", "torch"), idsOf(data));
        assertEquals(idsOf(data), idsOf(data.forVersion(3)));
        assertTrue(data.forVersion(3).require("ember").has("glow"));
    }

    @Test
    void everyOverlayWhoseRangeContainsTheVersionIsApplied() {
        AbstractData data = AbstractData.fromJson(OVERLAY_DOCUMENT);

        // Version 1 is covered by the overlays 1..1 and 1..2. Stopping at the
        // first match would keep ember's version 3 object.
        AbstractData v1 = data.forVersion(1);
        assertEquals(java.util.Arrays.asList("ember", "lantern", "torch"), idsOf(v1));
        assertEquals(3, v1.require("ember").getInt("tier"));
        assertEquals(40, v1.require("ember").getInt("legacy_tint"));
        assertEquals(200, v1.require("torch").getInt("legacy_tint"));

        // Version 2 is covered by 1..2 and 2..2, and ember loses only tier.
        AbstractData v2 = data.forVersion(2);
        assertEquals(java.util.Arrays.asList("ember", "lantern", "torch"), idsOf(v2));
        assertEquals(40, v2.require("ember").getInt("legacy_tint"));
        assertTrue(!v2.require("ember").has("tier"));
    }

    @Test
    void anOverlayMayAddAnIdTheBaseDoesNotHave() {
        AbstractData data = AbstractData.fromJson(OVERLAY_DOCUMENT);
        assertNull(data.get("lantern"));
        AbstractObject lantern = data.forVersion(1).require("lantern");
        assertEquals("Lantern", lantern.getString("name"));
        assertEquals(10, lantern.getInt("legacy_tint"));
    }

    @Test
    void removedIdsAreDeleted() {
        AbstractData data = AbstractData.fromJson(OVERLAY_DOCUMENT);
        assertNotNull(data.get("beacon"));
        assertNull(data.forVersion(1).get("beacon"));
        assertNull(data.forVersion(2).get("beacon"));
        assertNotNull(data.forVersion(3).get("beacon"));
    }

    @Test
    void aVersionOutsideTheDocumentsRangeIsRefused() {
        AbstractData data = AbstractData.fromJson(OVERLAY_DOCUMENT);
        assertThrows(AbstractDataException.class, () -> data.forVersion(0));
        assertThrows(AbstractDataException.class, () -> data.forVersion(4));
    }

    @Test
    void aDocumentWithNoOverlaysResolvesToItself() {
        AbstractData data = AbstractData.fromJson(
                "{\"abstract\": {\"format\": 1, \"compiler\": \"1.0.0\","
                        + " \"versions\": {\"min\": 1, \"max\": 1}},"
                        + " \"data\": [{\"template\": \"T\", \"id\": \"only\"}],"
                        + " \"overlays\": []}");
        assertEquals(idsOf(data), idsOf(data.forVersion(1)));
    }

    @Test
    void anUnknownDocumentFormatIsRefused() {
        assertThrows(AbstractDataException.class, () -> AbstractData.fromJson(
                "{\"abstract\": {\"format\": 2}, \"data\": [], \"overlays\": []}"));
    }

    // ------------------------------------------------------------------
    // Bundles
    // ------------------------------------------------------------------

    @Test
    void plainBundleWithKeyIsRefused() {
        byte[] bundle = plainBundle("{}".getBytes(StandardCharsets.UTF_8));

        assertEquals("{}", new String(
                AbstractBundle.openPayload(bundle, null), StandardCharsets.UTF_8));
        assertThrows(AbstractDataException.class,
                () -> AbstractBundle.openPayload(bundle, new byte[32]));
    }

    @Test
    void aClearedEncryptionFlagIsRefusedWithOrWithoutAKey() {
        // SPEC Appendix D.1 item 3: the refusal must not depend on the caller.
        // A plain container carries a checksum where a sealed one carries its
        // nonce, so clearing the flag never opens the container.
        byte[] bundle = plainBundle("{\"data\": []}".getBytes(StandardCharsets.UTF_8));
        bundle[6] ^= 0x40; // corrupt the plain checksum, as a cleared flag does

        assertThrows(AbstractDataException.class,
                () -> AbstractBundle.openPayload(bundle, null));
        assertThrows(AbstractDataException.class,
                () -> AbstractBundle.openPayload(bundle, new byte[32]));
    }

    /** Builds a plain ABX container the way the compiler writes one. */
    private static byte[] plainBundle(byte[] payload) {
        byte[] bundle = new byte[22 + payload.length];
        bundle[0] = 'A';
        bundle[1] = 'B';
        bundle[2] = 'X';
        bundle[3] = '1';
        bundle[5] = 1;
        int length = payload.length;
        bundle[18] = (byte) (length & 0xff);
        bundle[19] = (byte) ((length >>> 8) & 0xff);
        bundle[20] = (byte) ((length >>> 16) & 0xff);
        bundle[21] = (byte) ((length >>> 24) & 0xff);
        System.arraycopy(payload, 0, bundle, 22, payload.length);
        byte[] checksum = AbstractBundle.plainChecksum(bundle, payload);
        System.arraycopy(checksum, 0, bundle, 6, 12);
        return bundle;
    }
}
