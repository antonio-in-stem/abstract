# Abstract bundle security model

An `.abx` bundle packages compiled data. A sealed bundle uses authenticated
encryption with ChaCha20-Poly1305. It is optional; applications can consume
ordinary compiled JSON through `AbstractData.fromJson(...)`.

## Boundary

Encryption protects the plaintext from a party that does not have the key.
The loader checks authentication before accepting a sealed payload. These
properties do not prevent a recipient who controls the runtime from reading
keys or decrypted data, or replacing the loader.

Splitting a key across classes is not a confidentiality boundary when those
classes are delivered together. Obfuscation and key splitting have no measured
attacker-cost guarantee in this project. Bundles do not provide offline DRM,
license enforcement, provenance or protection against copying an intact file.

## Integration

New bundles require a random 256-bit key. Use `abstract keygen`; creation with
a human passphrase is refused. Legacy readers still accept the old SHA-256
passphrase mapping so existing files can be migrated. That mapping has no salt
or password-hardening cost and must not be used for new keys.

Compiler 1.5.0 samples each 96-bit nonce from the operating system's secure random
source. Failure to obtain randomness aborts sealing. Nonces do not depend on
the wall clock, process identity, payload or a resettable counter. For `q`
independent seals under one key, the nonce-collision bound is
`q(q - 1) / 2^97`. Limit use to at most 65,536 seals per key (collision probability
less than `2^-65`), then generate a new key. This is an operational limit, not
a counter enforced across processes. Prefer a new key for each delivery when
the application can manage it. A secure random source and independent draws
are assumptions; random nonces are not a proof of uniqueness.

The nonce requirement and consequences of reuse are specified in
[RFC 8439, section 4](https://www.rfc-editor.org/rfc/rfc8439.html#section-4).
An old file is not repaired by updating its reader. Rebuild it with a fresh
key if its nonce generation or passphrase strength is in doubt.

Rust uses RustCrypto implementations for SHA-256 and ChaCha20-Poly1305; Java
uses Bouncy Castle for the AEAD and the JVM's SHA-256 provider. Dependency
versions are recorded in `Cargo.lock` and `java/pom.xml`. Abstract does not
maintain its own implementations of these algorithms.

- Keep credentials and other secrets out of distributed data.
- Avoid logging keys and decrypted contents.
- Treat authentication failures as errors; do not fall back to unchecked data.
- Review `AbstractData` limits and the compiled-output contract before loading
  untrusted documents.

The Rust and Java implementations have algorithm-vector and malformed-input
checks. Passing those tests is not an independent cryptographic audit or proof
of hostile-host enforcement. See the tests and release evidence for the
versions and environments actually exercised.
