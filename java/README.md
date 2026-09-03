# Abstract Runtime for Java

Zero-dependency Java library that loads Abstract data into any JVM
application, including Minecraft plugins. Works on **Java 8 and newer**.

Two ways to ship data:

1. **Sealed bundles (recommended).** `abstract bundle` compiles your project
   and encrypts it into a single `.abx` file (ChaCha20-Poly1305, RFC 8439).
   The authored data never appears as plain text inside your jar, and any
   tampering fails loudly at load time. See `SECURITY.md` for the honest
   threat model.
2. **Plain JSON.** `abstract compile` output can be parsed directly with
   `AbstractData.fromJson(...)` when secrecy is not a concern.

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
    <version>0.2.0</version>
</dependency>
```

**Gradle (after `mvn install`):**

```groovy
repositories { mavenLocal() }
dependencies { implementation 'com.abstractlang:abstract-runtime:0.2.0' }
```

## Verifying without Maven

`src/selftest` contains a dependency-free checker that runs the RFC 8439
vectors and optionally opens a real bundle:

```bash
javac -d out $(find src -name '*.java')
java -cp out com.abstractlang.runtime.Selftest my-bundle.abx "my passphrase"
```

Expected output:

```
selftest: all RFC 8439 vectors and unit checks passed
selftest: opened bundle with N instance(s)
```
