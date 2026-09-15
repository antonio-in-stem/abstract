# Preserve the Abstract public API when using ProGuard in an application.
# These rules do not protect a distributed key or prevent data extraction.
# Keep the Bouncy Castle dependency and validate your final packaged artifact.
-keep class com.abstractlang.runtime.AbstractBundle { public *; }
-keep class com.abstractlang.runtime.AbstractData { public *; }
-keep class com.abstractlang.runtime.AbstractObject { public *; }
-keep class com.abstractlang.runtime.AbstractKeys { public *; }
-keep class com.abstractlang.runtime.AbstractDataException { public *; }
