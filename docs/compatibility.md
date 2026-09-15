# Compatibility

The compiler, language, editor, Java reader, and optional bundle format have
separate contracts. Current component values are declared in
[release-manifest.json](../release-manifest.json); do not copy them into a
consumer configuration or this guide.

The compiled document's `abstract.format` selects the decoder. The producer
string identifies the compiler that emitted it, not a new document format.
Model `versions` ranges belong to authored data and do not track releases.

The editor negotiates capabilities before using diagnostics, navigation,
renaming, or evaluated values. Consumers of the Java reader need only support
the compiled-data format they accept. See the [editor protocol](reference/editor-protocol.md)
and [compiled-data contract](reference/compiled-data.md).

## Bundles

An ABX1 bundle is an optional container around compiled JSON. A sealed bundle
authenticates and encrypts its contents; a plain bundle provides neither property.
New sealed bundles require a random 32-byte `hex:` key. Existing files using
the legacy passphrase derivation remain readable for migration, but new bundles
must use generated keys. Rebuild an old bundle with a fresh key when its key
handling is uncertain.

Wire contracts change only with an explicit compatibility decision and
regression evidence. A release update alone does not change document format
`1`, analysis protocol `1`, or model `versions` semantics.
