# Keep JNI entry points and their Kotlin declarations.
-keep class dev.phonealvr.client.NativeSession { *; }
-keepclasseswithmembernames class * {
    native <methods>;
}
