# VLESS

A VPN client for VLESS + XTLS Vision + REALITY servers (also VLESS + WebSocket + TLS). Android
first. The UI is DrawnUI for Rust; the tunnel is Android's VpnService running Xray-core
(AndroidLibXrayLite, its own TUN on the VPN's file descriptor).

## Build

`.\android.ps1 -Install` builds `target\android\drawnvless.apk` (arm64), installs it and starts it.
`-Bundle` also builds `target\android\drawnvless.aab` for Google Play. Release builds: Java shrunk
and renamed by R8 (`android\proguard-rules.pro` keeps what JNI calls by name), the Rust library
stripped. Signed with the debug key until the app gets its own upload key.

Needs the Android SDK (platform 36), NDK 26, a JDK 17+, cargo-ndk and the Rust Android targets.
The Xray-core library (`android\libs\libv2ray.aar`) is downloaded by the build script.

## Servers

Import a `vless://` link: copy it and tap Paste link, or open the link with the app.

## Layout

- `src/lib.rs`: the UI (main page and Settings in a SkiaShell), state, storage.
- `src/link.rs`: `vless://` parsing and the Xray config.
- `src/texts.rs`: English and Russian texts.
- `src/android.rs`: JNI to `MainActivity` and events back.
- `android/java`: `MainActivity` (GameActivity, VPN permission, clipboard, links) and
  `XrayVpnService` (the tunnel).

Language-switch restart audit: checked 2026-11-22, not affected (the language changes in place
through the app state, the UI is never rebuilt).
