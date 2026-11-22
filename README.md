# DrawnVpnClient

Android only for now.

A VPN client for VLESS + XTLS Vision + REALITY servers (also VLESS + WebSocket + TLS). The UI is
DrawnUI for Rust; the tunnel is Android's VpnService running Xray-core (AndroidLibXrayLite, its own
TUN on the VPN's file descriptor). App id `com.appomobi.drawnvpn`.

## Using it

- **Add a server:** copy its `vless://` link and tap Import, or open the link with the app. Without
  a new link on the clipboard, Import opens a field to paste or type one. A link for a server
  already in the list (same user, address and port) updates it.
- **Connect:** the big button, or tap a server: Connect, Disconnect, Test connection, Share (copies
  the link) and Delete.
- **Settings:** language (phone language, English, Russian) and the update check.

## Updates

At start and at most once a day the app asks GitHub for this repository's latest release. When it
is newer than the app, a banner offers its APK (Download opens it in the browser) or Later (that
version is not offered again). It can be turned off in Settings.

Releases: tag `v<version>` (the `version` in `Cargo.toml`, the same as `versionName`), with the APK
attached with a short name: `drawnvpn-<version without dots>-arm64.apk`, e.g. `drawnvpn-010-arm64.apk`
for 0.1.0. The app finds it by its `-arm64.apk` ending.

## Build

`.\android.ps1 -Install` builds `target\android\drawnvless.apk` (arm64), installs it and starts it.
`-Bundle` also builds `target\android\drawnvless.aab` for Google Play. Release builds: Java shrunk
and renamed by R8 (`android\proguard-rules.pro` keeps what JNI calls by name), the Rust library
stripped. Signed with the release key named in `androidkeystore.properties` (not in git: `storeFile`,
`keyAlias`, and `storePasswordEnv`, the environment variable holding the password); without that
file, with the debug key.

Needs the Android SDK (platform 36), NDK 26, a JDK 17+, cargo-ndk and the Rust Android targets.
The Xray-core library (`android\libs\libv2ray.aar`) is downloaded by the build script.

## Layout

- `src/lib.rs`: the UI (main page and Settings in a SkiaShell), state, storage.
- `src/link.rs`: `vless://` parsing and the Xray config.
- `src/update.rs`: the GitHub release check.
- `src/texts.rs`: English and Russian texts.
- `src/android.rs`: JNI to `MainActivity` and events back.
- `android/java`: `MainActivity` (GameActivity, VPN permission, clipboard, links, downloads) and
  `XrayVpnService` (the tunnel).

Language-switch restart audit: checked 2026-11-22, not affected (the language changes in place
through the app state, the UI is never rebuilt).
