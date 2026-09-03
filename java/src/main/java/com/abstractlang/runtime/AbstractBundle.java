package com.abstractlang.runtime;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;

/**
 * Opens {@code .abx} bundles produced by {@code abstract bundle}.
 *
 * <pre>{@code
 * byte[] key = AbstractKeys.combine(Keys.PART_A, Keys.PART_B);
 * AbstractData data = AbstractBundle.loadResource(MyPlugin.class, "/stickers.abx", key);
 * for (AbstractObject sticker : data.byTemplate("Sticker")) {
 *     registry.register(sticker.id(), sticker.getString("rarity"));
 * }
 * }</pre>
 *
 * <p>Bundle layout (little-endian): magic {@code ABX1}, flags byte,
 * payload-format byte, twelve bytes that hold the nonce when the container is
 * sealed and a keyless checksum when it is not, 4-byte payload length,
 * payload. Encrypted payloads use ChaCha20-Poly1305 with the 22-byte header as
 * associated data, so any modification (including header tampering and
 * encryption-flag downgrades) fails authentication.
 *
 * <p>A container whose encryption flag has been cleared never opens, with or
 * without a key (SPEC Appendix D.1 item 3). With a key it is refused as a
 * downgrade; without one, bytes 6..18 hold the AEAD nonce of the sealed
 * container it was made from, and so do not match the checksum a plain
 * container carries. A container that declares itself unencrypted opens
 * without a key and is refused with one.
 */
public final class AbstractBundle {

    private static final byte[] MAGIC = {'A', 'B', 'X', '1'};
    private static final int FLAG_ENCRYPTED = 0x01;
    private static final int FORMAT_JSON = 1;
    private static final int HEADER_LENGTH = 22;

    private AbstractBundle() {
    }

    /** Loads a bundle from raw bytes. Pass {@code null} for plain bundles. */
    public static AbstractData load(byte[] bundleBytes, byte[] key) {
        return AbstractData.fromJson(new String(openPayload(bundleBytes, key), StandardCharsets.UTF_8));
    }

    /** Loads a bundle from a stream (the stream is fully read, not closed). */
    public static AbstractData load(InputStream stream, byte[] key) {
        return load(readAll(stream), key);
    }

    /**
     * Loads a bundle embedded in the application jar, for example
     * {@code loadResource(MyPlugin.class, "/data/stickers.abx", key)}.
     */
    public static AbstractData loadResource(Class<?> owner, String resource, byte[] key) {
        InputStream stream = owner.getResourceAsStream(resource);
        if (stream == null) {
            throw new AbstractDataException("bundle resource not found: " + resource);
        }
        try {
            return load(stream, key);
        } finally {
            try {
                stream.close();
            } catch (IOException ignored) {
                // reading already finished; a close failure changes nothing
            }
        }
    }

    /** Decodes and (when sealed) decrypts the raw payload bytes. */
    public static byte[] openPayload(byte[] bytes, byte[] key) {
        if (bytes == null || bytes.length < HEADER_LENGTH) {
            throw new AbstractDataException("bundle is too small to contain an ABX header");
        }
        for (int i = 0; i < 4; i++) {
            if (bytes[i] != MAGIC[i]) {
                throw new AbstractDataException("not an ABX bundle (bad magic)");
            }
        }
        int flags = bytes[4] & 0xff;
        int format = bytes[5] & 0xff;
        if (format != FORMAT_JSON) {
            throw new AbstractDataException("unsupported bundle payload format " + format);
        }
        long declared = (bytes[18] & 0xffL)
                | (bytes[19] & 0xffL) << 8
                | (bytes[20] & 0xffL) << 16
                | (bytes[21] & 0xffL) << 24;
        int bodyLength = bytes.length - HEADER_LENGTH;
        if (declared != bodyLength) {
            throw new AbstractDataException("bundle payload length mismatch: header declares "
                    + declared + " bytes, found " + bodyLength);
        }

        boolean encrypted = (flags & FLAG_ENCRYPTED) != 0;
        if (!encrypted) {
            if (key != null) {
                // A caller holding a key expects sealed data; accepting a
                // "plain" bundle would enable a downgrade attack.
                throw new AbstractDataException(
                        "bundle claims to be unencrypted but a key was provided; refusing the downgrade");
            }
            byte[] payload = new byte[bodyLength];
            System.arraycopy(bytes, HEADER_LENGTH, payload, 0, bodyLength);
            // The same downgrade is refused without a key: a plain container
            // carries a keyless checksum where a sealed one carries its nonce.
            byte[] expected = plainChecksum(bytes, payload);
            int difference = 0;
            for (int i = 0; i < 12; i++) {
                difference |= expected[i] ^ bytes[6 + i];
            }
            if (difference != 0) {
                throw new AbstractDataException("bundle is not a valid plain container"
                        + " (checksum mismatch); it was modified or its encryption flag"
                        + " was cleared");
            }
            return payload;
        }

        if (key == null) {
            throw new AbstractDataException("bundle is encrypted; a key is required to open it");
        }
        byte[] nonce = new byte[12];
        System.arraycopy(bytes, 6, nonce, 0, 12);
        byte[] header = new byte[HEADER_LENGTH];
        System.arraycopy(bytes, 0, header, 0, HEADER_LENGTH);
        byte[] body = new byte[bodyLength];
        System.arraycopy(bytes, HEADER_LENGTH, body, 0, bodyLength);

        byte[] payload = ChaCha20Poly1305.open(key, nonce, header, body);
        if (payload == null) {
            throw new AbstractDataException(
                    "bundle authentication failed (wrong key or modified data)");
        }
        return payload;
    }

    /**
     * The keyless checksum a plain container carries in bytes 6..18: the first
     * twelve bytes of {@code SHA-256(header ‖ payload)}, computed with bytes
     * 6..18 of the header set to zero. {@code container} supplies the header;
     * only its first 22 bytes are read.
     *
     * <p>It is an integrity check, not an authentication tag — anyone can
     * recompute it — but it does prove that the container was written as a
     * plain container, which is what makes a cleared encryption flag
     * detectable without a key.
     */
    public static byte[] plainChecksum(byte[] container, byte[] payload) {
        if (container == null || container.length < HEADER_LENGTH) {
            throw new AbstractDataException("bundle is too small to contain an ABX header");
        }
        byte[] material = new byte[HEADER_LENGTH + payload.length];
        System.arraycopy(container, 0, material, 0, HEADER_LENGTH);
        for (int i = 6; i < 18; i++) {
            material[i] = 0;
        }
        System.arraycopy(payload, 0, material, HEADER_LENGTH, payload.length);

        byte[] digest;
        try {
            digest = MessageDigest.getInstance("SHA-256").digest(material);
        } catch (NoSuchAlgorithmException exception) {
            throw new AbstractDataException("SHA-256 is unavailable on this JVM", exception);
        }
        byte[] checksum = new byte[12];
        System.arraycopy(digest, 0, checksum, 0, 12);
        return checksum;
    }

    private static byte[] readAll(InputStream stream) {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[8192];
        try {
            int read;
            while ((read = stream.read(buffer)) != -1) {
                output.write(buffer, 0, read);
            }
        } catch (IOException exception) {
            throw new AbstractDataException("failed to read bundle stream", exception);
        }
        return output.toByteArray();
    }
}
