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
- **Downgrade resistance.** A loader that passes a key refuses plain
  (unencrypted) bundles, so an attacker cannot strip the encryption flag and
  substitute their own plaintext bundle.
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
- Key derivation: `SHA-256(passphrase)`, or exact bytes via `hex:<64 digits>`.
- Nonce: derived per seal from the payload digest, key, wall clock, and a
  process counter; identical data re-bundled still produces new ciphertext.
- Associated data: the full 22-byte header, binding magic, flags, format,
  nonce, and length.
- Tag comparison uses `MessageDigest.isEqual` (constant-time).

## Reporting

Found a weakness in the format or the implementations? Open an issue marked
`security` in the Abstract repository, or contact the maintainer privately.
