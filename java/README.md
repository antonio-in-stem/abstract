# Abstract Runtime for Java

Read compiled Abstract data in Java 8 or newer. The reader exposes typed
properties and reconstructs model versions from the document's overlays.

## Install from this repository

```sh
mvn -f java/pom.xml install
```

Add the locally installed library to your application:

```xml
<dependency>
    <groupId>com.abstractlang</groupId>
    <artifactId>abstract-runtime</artifactId>
    <version>1.1.0</version>
</dependency>
```

The library is not published to Maven Central. Maven resolves Bouncy Castle
as a transitive dependency for bundle cryptography. Include that dependency
when assembling your application's runtime classpath or distribution.

## Load compiled JSON

```java
import com.abstractlang.runtime.AbstractData;
import com.abstractlang.runtime.AbstractObject;

AbstractData data = AbstractData.fromJson(compiledJson);
for (AbstractObject product : data.byTemplate("Product")) {
    System.out.println(product.id() + ": " + product.getString("title"));
}
```

Compile the input with `abstract compile path/to/project JSON --out catalog.json`.
See the [compiled data contract](../docs/raw-data.md) for the envelope and
resource-path semantics.

| Call | Result |
| --- | --- |
| `getString(path)` | Text; numbers and booleans are stringified |
| `getInt(path)` | `long` |
| `getFloat(path)` | `double` |
| `getBool(path)` | `boolean` |
| `getObject(path)` | Nested object or `null` |
| `getObjects(path)` | List of objects |
| `getStrings(path)` / `getList(path)` | Lists |
| `has(path)` / `raw(path)` | Presence or raw value |

Scalar accessors also accept a fallback. Paths can address nested values, such
as `owner.team`. Collections support `all()`, `get(id)`, `require(id)`,
`byTemplate(name)`, `size()` and iteration.

Malformed JSON raises `AbstractDataException`. The parser checks nesting depth,
duplicate keys, number syntax, Unicode escapes and control characters.

## Read a model version

```java
AbstractData previous = data.forVersion(2);
```

`data` contains the maximum model version. `forVersion` applies every matching
overlay, including additions and removals, and returns another `AbstractData`.
A version outside `minVersion()..maxVersion()` raises `AbstractDataException`.
Model versions are independent of compiler and runtime releases.

## Optional encrypted bundles

A bundle packages compiled JSON in an ABX1 container. It does not change
validation or package the bytes of referenced resources.

```sh
key="$(abstract keygen)"
abstract bundle path/to/project --key "$key" --out catalog.abx
```

This shell example captures a new key. Store it securely before ending the
session, outside source control and build logs. Command-line arguments can be visible to other processes under
the same account; the application is responsible for secure key storage.

```java
import com.abstractlang.runtime.AbstractBundle;
import com.abstractlang.runtime.AbstractKeys;

byte[] key = AbstractKeys.fromHex(storedHexKey);
AbstractData data = AbstractBundle.loadResource(
    MyApplication.class, "/data/catalog.abx", key);
```

A sealed bundle is accepted only after authentication succeeds. `--plain`
creates an unencrypted container with a checksum; it provides no authenticity.
Supplying a key for a plain container is refused.

Existing passphrase-based bundles remain readable through
`AbstractKeys.fromKeyMaterial(...)`. That legacy derivation is unsuitable for
new keys. See [compatibility and migration](../docs/COMPATIBILITY.md).

Encryption depends on keeping the key secret. Delivering the key with the
application lets a recipient who controls that runtime recover the plaintext.
Splitting the key across classes does not change that boundary. Read the
[bundle security model](SECURITY.md) before choosing this optional format.

## Verify changes

```sh
mvn -B -f java/pom.xml verify
```

JUnit covers the reader, overlay behavior, authenticated encryption and invalid
containers. Set `ABSTRACT_COMPILER_PATH` to a built compiler to also exercise
Rust-produced bundles with the Java reader; CI sets it on Java 8 and Java 21.
