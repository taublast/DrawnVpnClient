# Called by name from Rust (src/android.rs) and from native code.
-keep class com.appomobi.drawnvpn.MainActivity {
    public void vpnConnect(java.lang.String, java.lang.String);
    public void vpnDisconnect();
    public void vpnTest();
    public java.lang.String clipboardText();
    public void copyText(java.lang.String);
    public void fetchText(java.lang.String);
    public void openUrl(java.lang.String);
    public java.lang.String systemLanguage();
    public java.lang.String coreVersion();
    native <methods>;
}
# GameActivity and GameTextInput: android-activity calls their methods and fields through JNI.
-keep class com.google.androidgamesdk.** { *; }
# Xray-core (gomobile): the Go side finds these classes and methods by name.
-keep class go.** { *; }
-keep class libv2ray.** { *; }
-keepclasseswithmembernames class * { native <methods>; }
