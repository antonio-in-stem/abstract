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

- Keep credentials and other secrets out of distributed data.
- Avoid logging keys and decrypted contents.
- Treat authentication failures as errors; do not fall back to unchecked data.
- Review `AbstractData` limits and the compiled-output contract before loading
  untrusted documents.

The Rust and Java implementations have algorithm-vector and malformed-input
checks. Passing those tests is not an independent cryptographic audit or proof
of hostile-host enforcement. See the tests and release evidence for the
versions and environments actually exercised.
