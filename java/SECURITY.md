# Abstract Bundle Security Model

This document explains exactly what `.abx` bundles protect, what they do not
protect, and how to get the most protection in a Java application. It is
written to be honest: read the threat model before relying on it.

## What a sealed bundle gives you

- **Confidentiality at rest.** The compiled data is encrypted with
  ChaCha20-Poly1305 (RFC 8439). Unzipping the jar, running `strings`, or
  opening the bundle in a hex editor reveals nothing about the authored data.
- **Tamper evidence.** Every byte of the payload and the 22-byte header is
  authenticated. Editing the ciphertext, the header flags, or the declared
  length makes loading fail with a clear error instead of producing corrupted
  data.
- **Downgrade resistance, whatever the caller does.** Stripping the encryption
  flag does not produce a bundle that opens. A loader holding a key refuses a
  bundle that claims to be plain, and a loader with no key finds that the
  twelve header bytes which hold an AEAD nonce in a sealed bundle do not match
  the keyless checksum a genuine plain bundle carries there. Neither refusal
  depends on how the caller was written, which is the property that matters:
  a rule enforced only when the caller passes a key is a rule an attacker gets
  to opt out of.
- **Format stability.** The implementation is verified against the official
  RFC 8439 and FIPS 180-4 test vectors on both the Rust (encoder) and Java
  (decoder) sides.

## What a sealed bundle cannot give you

Be clear-eyed about this part:

- **The key ships with the application.** Anyone who fully reverses your jar
  can eventually recover the key and decrypt the bundle. Obfuscation raises
  the cost of that attack from minutes to hours or days; it does not make it
  impossible. This is the same limit every DRM system has.
- **Runtime memory is readable.** A debugger attached to the JVM can read the
  decrypted data after loading. Protecting against a user with full control
  of their own machine is not a solvable problem.
- **It is not a licensing system.** If you need to gate features per customer,
  do that check on a server you control.

Design rule: put data in bundles that you would be *annoyed* to see extracted,
not data that would be *catastrophic* to see extracted. Catastrophic data
(economy balancing you sell, private endpoints, credentials) belongs on a
server.

## Hardening checklist for Java applications

1. **Never store the key as one constant.** Split it into XOR shares at build
   time and place each share in a different class:

   ```java
   // Build-time (run once, paste the output):
   byte[] key = AbstractKeys.fromPassphrase("your real passphrase");
   for (byte[] share : AbstractKeys.split(key, 3)) {
       System.out.println(AbstractKeys.toJavaInitializer(share));
   }

   // Runtime (shares live in three unrelated classes):
   byte[] key = AbstractKeys.combine(KeyA.PART, KeyB.PART, KeyC.PART);
   AbstractData data = AbstractBundle.loadResource(Plugin.class, "/data.abx", key);
   ```

   A single share reveals nothing; the attacker must find and combine all of
   them, which ProGuard renaming makes considerably harder.

2. **Run ProGuard (or a commercial obfuscator) on release builds.** The
   included `proguard-rules.pro` keeps the runtime working while renaming
   your classes, collapsing the key-share holders into meaningless names.
   Commercial tools (DexGuard, Zelix KlassMaster, Allatori) add string
   encryption and control-flow obfuscation on top; use one if the data value
   justifies the license cost.

3. **Do not log decrypted content.** The most common leak is a debug log
   line, not cryptanalysis.

4. **Load once, hold references.** Decrypt at startup into your registry and
   let the byte arrays go out of scope. Avoid re-reading the bundle on every
   request.

5. **Rotate keys when shipping major versions.** Re-bundle with a new
   passphrase per release so an old leaked key does not open the new data.

6. **Consider a server-supplied share.** For online applications, fetch one
   XOR share from your backend at startup. The jar alone then contains
   insufficient material to decrypt the bundle offline.

## Cipher and format details

- Cipher: ChaCha20-Poly1305 AEAD, 32-byte key, 12-byte nonce, 16-byte tag.
- Key derivation: `SHA-256(passphrase)`. Only a `hex:` prefix selects raw key
  bytes; a bare 64-character hexadecimal string is a passphrase and is hashed,
  on the compiler side and in `AbstractKeys.fromKeyMaterial` alike.
- Container layout: magic `ABX1`, a flags byte, a payload-format byte, twelve
  bytes, a 4-byte little-endian payload length, then the payload. The twelve
  bytes hold the AEAD nonce in a sealed container and the plain checksum in an
  unencrypted one.
- Associated data: the full 22-byte header, binding magic, flags, format,
  nonce, and length.
- Tag comparison uses `MessageDigest.isEqual` (constant-time).

### Nonce construction

Each seal derives its nonce as the first twelve bytes of

```
SHA-256( SHA-256(payload) || key || clock || counter )
```

where `clock` is the wall clock in nanoseconds since the Unix epoch as sixteen
little-endian bytes (sixteen zero bytes if the clock is before the epoch) and
`counter` is a process-local 64-bit counter as eight little-endian bytes.

The counter starts at zero and **is reset by exactly one event: the start of a
new process.** It is never reset while the process runs, it is shared by every
thread in that process, and it is not persisted. So two seals in one process
always differ, even when the clock does not advance; across processes the
counters repeat and uniqueness rests on the clock and on the payload and key
digests being mixed in. Re-bundling identical data with the same key therefore
still produces fresh ciphertext.

### Plain checksum

An unencrypted container carries, in the twelve bytes a sealed container uses
for its nonce, the first twelve bytes of `SHA-256(header || payload)` computed
with those same twelve bytes zeroed.

This is an integrity check and **not** an authentication tag: anyone can
recompute it, so it proves nothing about who wrote the container. What it does
prove is that the container was *written as* a plain container, which is why
clearing a sealed container's encryption flag is detectable with no key in
hand. Do not read a passing checksum as evidence of provenance; a plain
container offers no confidentiality and no authenticity, which is why it is
for debugging and not for shipping.

## Reporting

Found a weakness in the format or the implementations? Open an issue marked
`security` in the Abstract repository, or contact the maintainer privately.
