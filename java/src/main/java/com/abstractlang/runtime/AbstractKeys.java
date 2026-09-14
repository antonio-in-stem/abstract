package com.abstractlang.runtime;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.security.SecureRandom;

/**
 * Key material helpers, mirroring the Abstract CLI:
 *
 * <ul>
 *   <li>{@link #fromPassphrase(String)} hashes any passphrase with SHA-256,
 *       exactly like {@code abstract bundle --key "my passphrase"}.</li>
 *   <li>{@link #fromHex(String)} takes the 64-digit form used with
 *       {@code --key hex:...}.</li>
 *   <li>{@link #combine(byte[]...)} XORs key shares back together. Store
 *       the shares in different classes (see {@link #split}) so the real
 *       key never appears as one constant in the compiled jar.</li>
 * </ul>
 */
public final class AbstractKeys {

    private AbstractKeys() {
    }

    /** Derives the 32-byte bundle key from a passphrase (SHA-256). */
    public static byte[] fromPassphrase(String passphrase) {
        if (passphrase == null || passphrase.trim().isEmpty()) {
            throw new AbstractDataException("bundle keys must not be empty");
        }
        try {
            return MessageDigest.getInstance("SHA-256")
                    .digest(passphrase.getBytes(StandardCharsets.UTF_8));
        } catch (NoSuchAlgorithmException exception) {
            throw new AbstractDataException("SHA-256 is unavailable on this JVM", exception);
        }
    }

    /**
     * Derives a key from CLI key material, by exactly the rule the compiler
     * uses (SPEC Appendix D.2 item 10): only the {@code hex:} prefix selects
     * raw key bytes, and everything else — including a bare 64-character
     * hexadecimal string — is a passphrase and is hashed.
     *
     * <p>This is the method to use when the key material comes from a config
     * file or a command line, so that a given {@code --key} value means the
     * same thing on both sides of the bundle.
     */
    public static byte[] fromKeyMaterial(String material) {
        if (material == null) {
            throw new AbstractDataException("bundle keys must not be null");
        }
        if (material.startsWith("hex:")) {
            return fromHex(material);
        }
        return fromPassphrase(material);
    }

    /**
     * Parses a 64-hex-digit key (with or without the {@code hex:} prefix).
     *
     * <p>{@code null} and any non-hexadecimal digit surface as
     * {@link AbstractDataException}, never as {@code NullPointerException} or
     * {@code NumberFormatException} (SPEC Appendix D.2 item 9), so a caller
     * handing user input to this method catches one exception type, not three.
     *
     * <p>To honour the compiler's own rule that a bare hexadecimal string is a
     * passphrase, use {@link #fromKeyMaterial(String)} instead.
     */
    public static byte[] fromHex(String hex) {
        if (hex == null) {
            throw new AbstractDataException("hex keys must not be null");
        }
        String digits = hex.startsWith("hex:") ? hex.substring(4) : hex;
        if (digits.length() != 64) {
            throw new AbstractDataException("hex keys must contain exactly 64 hexadecimal digits;"
                    + " this one has " + digits.length());
        }
        byte[] key = new byte[32];
        for (int i = 0; i < 32; i++) {
            int high = hexValue(digits.charAt(i * 2));
            int low = hexValue(digits.charAt(i * 2 + 1));
            if (high < 0 || low < 0) {
                throw new AbstractDataException("hex keys must contain exactly 64 hexadecimal"
                        + " digits; '" + digits.substring(i * 2, i * 2 + 2) + "' is not one");
            }
            key[i] = (byte) ((high << 4) | low);
        }
        return key;
    }

    private static int hexValue(char digit) {
        if (digit >= '0' && digit <= '9') {
            return digit - '0';
        }
        if (digit >= 'a' && digit <= 'f') {
            return digit - 'a' + 10;
        }
        if (digit >= 'A' && digit <= 'F') {
            return digit - 'A' + 10;
        }
        return -1;
    }

    /**
     * XOR-combines key shares produced by {@link #split}. All shares must be
     * present and 32 bytes long; any single share reveals nothing about the
     * key on its own.
     */
    public static byte[] combine(byte[]... shares) {
        if (shares == null || shares.length == 0) {
            throw new AbstractDataException("combine needs at least one key share");
        }
        byte[] key = new byte[32];
        for (byte[] share : shares) {
            if (share == null || share.length != 32) {
                throw new AbstractDataException("every key share must be exactly 32 bytes");
            }
            for (int i = 0; i < 32; i++) {
                key[i] ^= share[i];
            }
        }
        return key;
    }

    /**
     * Splits a key into {@code count} XOR shares (build-time helper). Print
     * the shares as Java byte-array constants and place each in a different
     * class; recombine at runtime with {@link #combine}.
     */
    public static byte[][] split(byte[] key, int count) {
        if (key == null || key.length != 32) {
            throw new AbstractDataException("keys must be exactly 32 bytes");
        }
        if (count < 2) {
            throw new AbstractDataException("split needs at least 2 shares");
        }
        SecureRandom random = new SecureRandom();
        byte[][] shares = new byte[count][32];
        byte[] accumulator = key.clone();
        for (int index = 0; index < count - 1; index++) {
            random.nextBytes(shares[index]);
            for (int i = 0; i < 32; i++) {
                accumulator[i] ^= shares[index][i];
            }
        }
        shares[count - 1] = accumulator;
        return shares;
    }

    /** Formats a share as a Java initializer, for pasting into source. */
    public static String toJavaInitializer(byte[] share) {
        StringBuilder builder = new StringBuilder("new byte[] {");
        for (int i = 0; i < share.length; i++) {
            if (i > 0) {
                builder.append(", ");
            }
            builder.append("(byte) 0x").append(String.format("%02x", share[i]));
        }
        return builder.append("}").toString();
    }
}
