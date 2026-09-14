# Abstract Runtime for Java

Zero-dependency Java library that loads compiled Abstract data into a JVM
application, including Minecraft plugins. Works on **Java 8 and newer**.

It reads the compiled document envelope of SPEC 8.1 - `abstract`, `data` and
`overlays` - and gives you any version of the project through
`AbstractData.forVersion(int)`.

Two ways to ship data:

1. **Compiled JSON.** Read normal `abstract compile` output directly with
   `AbstractData.fromJson(...)`.
2. **Optional bundles.** `abstract bundle` packages compiled data in `.abx`.
   Sealed bundles use authenticated encryption. Their confidentiality depends
   on keeping the key secret; a recipient-controlled JVM can expose keys and
   decrypted data. See [the security model](SECURITY.md).

Both give you an `AbstractData`, and both refuse malformed input with
`AbstractDataException` rather than a JVM error. The JSON reader is strict on
purpose: it enforces a nesting-depth limit of 64, rejects a leading `+`, a
leading zero, duplicate object keys, lone surrogates and unescaped control
characters, and reports a malformed `\u` escape as
`AbstractDataException` rather than `NumberFormatException`.

## Build the bundle (CLI side)

```bash
abstract bundle path/to/project --key "my release passphrase" --out stickers.abx
```

Put `stickers.abx` in your plugin resources (for example
`src/main/resources/data/stickers.abx`).

## Load the bundle (Java side)

```java
import com.abstractlang.runtime.AbstractBundle;
import com.abstractlang.runtime.AbstractData;
import com.abstractlang.runtime.AbstractKeys;
import com.abstractlang.runtime.AbstractObject;

public final class StickerRegistry {

    public void loadAll() {
        byte[] key = AbstractKeys.combine(KeyA.PART, KeyB.PART, KeyC.PART);
        AbstractData data = AbstractBundle.loadResource(
                StickerRegistry.class, "/data/stickers.abx", key);

        for (AbstractObject sticker : data.byTemplate("Sticker")) {
            register(
                    sticker.id(),
                    sticker.getString("rarity"),
                    sticker.getInt("variant_count", 0),
                    sticker.getStrings("flags"));

            for (AbstractObject entry : sticker.getObjects("lang_values")) {
                addTranslation(sticker.id(),
                        entry.getString("key"), entry.getString("value"));
            }

            String slotMode = sticker.getString("slots.1.mode", "generate");
        }
    }
}
```

Typed access summary:

| Call | Returns |
| --- | --- |
| `getString("a.b")` / `getString(path, fallback)` | text (numbers/bools stringified) |
| `getInt(path)` / `getInt(path, fallback)` | `long` |
| `getFloat(path)` / `getFloat(path, fallback)` | `double` |
| `getBool(path)` / `getBool(path, fallback)` | `boolean` |
| `getObject(path)` | nested `AbstractObject` or `null` |
| `getObjects(path)` | list of objects (tagged group lists) |
| `getStrings(path)` / `getList(path)` | lists |
| `has(path)` / `raw(path)` | presence / raw value |

Collection access: `data.all()`, `data.get(id)`, `data.require(id)`,
`data.byTemplate(name)`, `data.size()`, iteration.

## Versions

A 1.0 project declares a range of versions, and one compiled document carries
all of them: `data` holds the objects of the **maximum** version, and
`overlays` holds every earlier version that differs from it (SPEC 7.5).

```java
AbstractData data = AbstractBundle.loadResource(Plugin.class, "/data.abx", key);

System.out.println(data.minVersion() + ".." + data.maxVersion());  // e.g. 1..3
AbstractData forV2 = data.forVersion(2);
```

`forVersion(int)` applies **every** overlay whose range contains the version -
there may be none, one or several - replacing or adding objects by `id` and
then deleting the ids the overlay lists as `removed`. Two consequences are
worth knowing before you write your own reader:

- more than one overlay can cover one version, so stopping at the first match
  reads the wrong document;
- an id can appear in an overlay and in no base document at all, when an
  instance's window ends before the maximum version.

The result is ordered by `(template, id)` like the base, and carries no
overlays of its own. A version outside `minVersion()..maxVersion()` is an
`AbstractDataException`.

`forVersion` returns another `AbstractData`, indexed the same way, so every
collection accessor above works on it unchanged.

## Key handling

Never keep the passphrase or the final key as a single constant. Generate
XOR shares once at build time and spread them across classes:

```java
byte[] key = AbstractKeys.fromPassphrase("my release passphrase");
for (byte[] share : AbstractKeys.split(key, 3)) {
    System.out.println(AbstractKeys.toJavaInitializer(share));
}
```

Then recombine at runtime with `AbstractKeys.combine(...)`. Details and the
full hardening checklist live in `SECURITY.md`; ProGuard configuration lives
in `proguard-rules.pro`.

## Installing the library

**Option A: copy the sources.** The runtime is seven small files with no
dependencies. Copy `src/main/java/com/abstractlang/runtime/` into your
project (relocate the package if you prefer). This is the simplest option
for Minecraft plugins and it lets ProGuard rename everything.

**Option B: Maven local install.**

```bash
mvn install          # from this directory
```

```xml
<dependency>
    <groupId>com.abstractlang</groupId>
    <artifactId>abstract-runtime</artifactId>
    <version>1.0.0</version>
</dependency>
```

**Gradle (after `mvn install`):**

```groovy
repositories { mavenLocal() }
dependencies { implementation 'com.abstractlang:abstract-runtime:1.0.0' }
```

## Verifying without Maven

`src/selftest` contains a dependency-free checker that runs the RFC 8439
vectors and optionally opens a real bundle:

```bash
javac -d out $(find src/main src/selftest -name '*.java')
java -cp out com.abstractlang.runtime.Selftest my-bundle.abx "my passphrase"
```

The bundle path and passphrase are optional; without them the checker runs
only the vectors and unit checks. Expected output:

```
selftest: all RFC 8439 vectors and unit checks passed
selftest: opened bundle with N instance(s)
```

The checker covers the RFC 8439 vectors, the key-material rules, the strict
JSON reader, the version overlays, and the container rules below. The same
ground is covered by JUnit in `src/test`, which needs the JUnit 5 jars and so
needs Maven or another runner.

## Plain and sealed containers

- A container that declares itself **unencrypted** opens with no key. Passing
  `--key` (or a non-null key to `openPayload`) against one is refused: a key
  is never silently discarded.
- A container whose **encryption flag has been cleared** never opens, with or
  without a key. With a key it is refused as a downgrade; without one it fails
  a keyless checksum that a genuine plain container carries in the twelve
  header bytes where a sealed container carries its nonce. The refusal does
  not depend on what the caller passes.
- Key material follows the compiler's rule exactly: only a `hex:` prefix
  selects raw key bytes, and everything else - including a bare 64-character
  hexadecimal string - is a passphrase. `AbstractKeys.fromKeyMaterial(String)`
  implements that rule; `fromHex` and `fromPassphrase` are the explicit forms,
  and both refuse `null` and non-hexadecimal digits with
  `AbstractDataException`.
