# DrawnVpnClient (Net Connect)

Android only for now.

A VPN client for VLESS + XTLS Vision + REALITY servers (also VLESS + WebSocket + TLS). The UI is
DrawnUI for Rust; the tunnel is Android's VpnService running Xray-core (AndroidLibXrayLite, its own
TUN on the VPN's file descriptor). App id `com.appomobi.drawnnetc`.

## Using it

- **Add a server:** copy its `vless://` link and tap Import. Without a new link on the clipboard,
  Import opens a field to paste or type one. The app declares no `vless://` link handler on
  purpose: other apps could ask Android which app opens such links. A link for a server
  already in the list (same user, address and port) updates it.
- **Connect:** the big button, or tap a server: Connect, Disconnect, Test connection, Share (copies
  the link) and Delete.
- **Settings:** language (phone language, English, Russian) and the update check.

## Updates

At start and at most once a day the app asks GitHub for this repository's latest release. When it
is newer than the app, a banner offers its APK (Download opens it in the browser) or Later (that
version is not offered again). It can be turned off in Settings.

Releases: tag `v<version>` (the `version` in `Cargo.toml`, the same as `versionName`), with two
APKs under short names: `netconnect-010-arm64.apk` (64-bit phones) and `netconnect-010-armv7.apk`
(older 32-bit phones) for 0.1.0. The app looks for the one ending like its own build.

## Build

`.\android.ps1 -Install` builds `target\android\drawnnetc.apk` (arm64), installs it and starts it.
`.\android.ps1 -Release` builds the release files: `target\android\netconnect-010-arm64.apk`,
`netconnect-010-armv7.apk` and `netconnect-010.aab` (both, for Google Play). Release builds: Java shrunk
and renamed by R8 (`android\proguard-rules.pro` keeps what JNI calls by name), the Rust library
stripped. Signed with the release key named in `android\keystore.properties` (not in git: `storeFile`,
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
  `ConnectionService` (the tunnel).

Language-switch restart audit: checked 2026-11-22, not affected (the language changes in place
through the app state, the UI is never rebuilt).
