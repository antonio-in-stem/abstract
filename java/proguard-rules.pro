# ProGuard rules for applications embedding the Abstract runtime.
#
# Goal: keep the runtime functional while letting ProGuard rename and
# optimize everything else, including the classes that hold your key shares.
#
# Typical usage in a Gradle build:
#   minifyEnabled true / proguard task with:
#   -include proguard-rules.pro

# --- Required: keep the public runtime API usable from your code ----------
# (ProGuard will still rename internal classes such as ChaCha20Poly1305 and
# Json when you repackage; the API surface below is what your plugin calls.)
-keep class com.abstractlang.runtime.AbstractBundle { public *; }
-keep class com.abstractlang.runtime.AbstractData { public *; }
-keep class com.abstractlang.runtime.AbstractObject { public *; }
-keep class com.abstractlang.runtime.AbstractKeys { public *; }
-keep class com.abstractlang.runtime.AbstractDataException { public *; }

# --- Recommended hardening -------------------------------------------------
# Move the runtime (and your key-share classes) into a flat, meaningless
# package so class names stop hinting at their purpose.
-repackageclasses ''
-allowaccessmodification

# Aggressive optimization passes make decompiled control flow harder to read.
-optimizationpasses 5
-overloadaggressively

# Remove debug attributes that make reversing comfortable.
# (Keep line numbers only if you need readable crash reports; if so, also
# ship a mapping.txt and keep it private.)
-renamesourcefileattribute ''

# Do NOT add -keep rules for the classes that hold your key shares.
# They should be renamed into the anonymous soup with everything else.

# --- Notes ------------------------------------------------------------------
# 1. ProGuard does not encrypt strings. Your passphrase must never appear as
#    a string literal; store XOR shares as byte arrays (see SECURITY.md).
# 2. For string encryption and control-flow obfuscation, layer a commercial
#    obfuscator (DexGuard, Zelix, Allatori) after ProGuard.
# 3. Always test the obfuscated jar: run your plugin's startup path and
#    verify the bundle loads before shipping.
