package com.abstractlang.runtime;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

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
 * payload-format byte, 12-byte nonce, 4-byte payload length, payload.
 * Encrypted payloads use ChaCha20-Poly1305 with the 22-byte header as
 * associated data, so any modification (including header tampering and
 * encryption-flag downgrades) fails authentication.
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
