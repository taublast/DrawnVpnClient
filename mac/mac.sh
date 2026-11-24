#!/bin/sh
# macOS, on a Mac with Xcode and Go 1.27+: builds the app (Rust, DrawnUI) and its connection system
# extension (Swift + the engine: Xray-core as a Go C archive), both universal (Apple silicon + Intel),
# bundles them, signs both.
#   mac/mac.sh            build, sign (development), install into /Applications, open
#   mac/mac.sh build      build and sign only
#   mac/mac.sh release    sign with Developer ID, notarize, staple, zip:
#                         target/.../macos/netconnect-<version without dots>-macos.zip
# A system extension runs only from /Applications; the first connect asks the user to allow it in
# System Settings > Privacy & Security, then to allow the VPN configuration.
#
# Signing, from the environment or from mac/signing.properties (KEY=value lines, not in git):
#   MAC_IDENTITY        codesigning identity; default: the first "Apple Development" one
#                       (release: the first "Developer ID Application" one)
#   MAC_APP_PROFILE     macOS provisioning profile for com.appomobi.drawnnetc
#   MAC_EXT_PROFILE     macOS provisioning profile for com.appomobi.drawnnetc.connection
#   MAC_NOTARY_PROFILE  release: the keychain profile of `xcrun notarytool store-credentials`
# Both profiles need the System Extension and Network Extensions capabilities and an app group;
# without a path the newest installed profile for that exact id (and this Mac, for development)
# is used.
set -eu
cd "$(dirname "$0")/.."
root="$PWD"

crate=drawnnetc
name="Net Connect"
bundle_id=com.appomobi.drawnnetc
ext_id=$bundle_id.connection
min_macos=13.0   # Go 1.27 (the engine) builds for macOS 13 and later

action=run
for arg in "$@"; do
    case "$arg" in
        build) action=build ;;
        run) action=run ;;
        release) action=release ;;
        *) echo "unknown option $arg"; exit 1 ;;
    esac
done

if [ -f mac/signing.properties ]; then
    while IFS='=' read -r key value; do
        case "$key" in
            MAC_IDENTITY|MAC_APP_PROFILE|MAC_EXT_PROFILE|MAC_NOTARY_PROFILE)
                eval "current=\${$key:-}"
                [ -n "$current" ] || export "$key=$value" ;;
        esac
    done < mac/signing.properties
fi

sdk_path=$(xcrun --sdk macosx --show-sdk-path)
target_dir=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
work="$target_dir/macos"
mkdir -p "$work"
export MACOSX_DEPLOYMENT_TARGET=$min_macos

# 1. The app: Rust for both architectures (the glue is compiled by build.rs), one universal binary.
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target add "$target" >/dev/null 2>&1 || true
    cargo build --target "$target" --release
done
lipo -create "$target_dir/aarch64-apple-darwin/release/$crate" "$target_dir/x86_64-apple-darwin/release/$crate" -output "$work/$crate"

# 2. The engine (Xray-core, ios/engine) for both architectures, and 3. the extension linked with it.
engine_version=$(sed -n 's/.*Xray-core v\([0-9.]*\).*/\1/p' ios/engine/go.mod | head -1)
clang=$(xcrun --sdk macosx -f clang)
for arch in arm64 x86_64; do
    goarch=$arch
    [ "$arch" = x86_64 ] && goarch=amd64
    flags="-isysroot $sdk_path -arch $arch -mmacosx-version-min=$min_macos"
    (
        cd ios/engine
        # Xray-core needs Go 1.27; an older go on PATH fetches it for this build (GOTOOLCHAIN).
        GOTOOLCHAIN="${GOTOOLCHAIN_ENGINE:-go1.27.3}" GOOS=darwin GOARCH=$goarch CGO_ENABLED=1 CC="$clang" \
            CGO_CFLAGS="$flags" CGO_LDFLAGS="$flags" \
            go build -buildmode=c-archive -trimpath -ldflags="-s -w -buildid=" -o "$work/libengine-$arch.a" .
    )
    xcrun --sdk macosx swiftc -O -target "$arch-apple-macos$min_macos" -sdk "$sdk_path" -module-name Connection \
        -import-objc-header ios/connection/Engine.h ios/connection/Provider.swift mac/connection/main.swift \
        "$work/libengine-$arch.a" -framework NetworkExtension -framework Security -framework CoreFoundation -lresolv \
        -o "$work/connection-$arch"
done
lipo -create "$work/connection-arm64" "$work/connection-x86_64" -output "$work/$ext_id"

# 4. Signing inputs: identity, profiles, and the app group the profiles grant.
if [ "$action" = release ]; then
    identity="${MAC_IDENTITY:-$(security find-identity -v -p codesigning | grep -m1 'Developer ID Application' | sed -E 's/.*"(.*)"/\1/')}"
    [ -n "$identity" ] || { echo "no Developer ID Application identity: see the top of this script"; exit 1; }
else
    identity="${MAC_IDENTITY:-$(security find-identity -v -p codesigning | grep -m1 'Apple Development' | sed -E 's/.*"(.*)"/\1/')}"
    [ -n "$identity" ] || { echo "no Apple Development identity"; exit 1; }
fi
mac_udid=$(system_profiler SPHardwareDataType | sed -n 's/.*Provisioning UDID: //p' | head -1)

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The newest macOS profile for exactly this id: development ones must list this Mac, Developer ID
# ones (release) cover every Mac.
find_profile() {
    python3 - "$1" "$mac_udid" "$action" <<'PY'
import glob, os, plistlib, subprocess, sys, datetime
bundle_id, udid, action, best = sys.argv[1], sys.argv[2], sys.argv[3], None
folders = ["~/Library/Developer/Xcode/UserData/Provisioning Profiles", "~/Library/MobileDevice/Provisioning Profiles"]
for path in (p for f in folders for p in glob.glob(os.path.join(os.path.expanduser(f), "*.provisionprofile"))):
    try:
        p = plistlib.loads(subprocess.run(["security", "cms", "-D", "-i", path], capture_output=True).stdout)
    except Exception:
        continue
    e, team = p.get("Entitlements", {}), (p.get("TeamIdentifier") or [""])[0]
    if "OSX" not in p.get("Platform", []) or e.get("com.apple.application-identifier") != f"{team}.{bundle_id}":
        continue
    if p["ExpirationDate"] < datetime.datetime.now():
        continue
    if action == "release":
        if not p.get("ProvisionsAllDevices"):
            continue
    elif udid not in p.get("ProvisionedDevices", []):
        continue
    if best is None or p["CreationDate"] > best[0]:
        best = (p["CreationDate"], path)
print(best[1] if best else "")
PY
}
app_profile="${MAC_APP_PROFILE:-$(find_profile "$bundle_id")}"
ext_profile="${MAC_EXT_PROFILE:-$(find_profile "$ext_id")}"
[ -n "$app_profile" ] || { echo "no macOS profile for $bundle_id ($action): see the top of this script"; exit 1; }
[ -n "$ext_profile" ] || { echo "no macOS profile for $ext_id ($action): see the top of this script"; exit 1; }

# Entitlements from a profile: the tunnel (the system extension's kind when the profile has it),
# the app group, installing the extension (the app), debugging (development).
entitlements() {
    security cms -D -i "$1" > "$tmp/profile.plist"
    python3 - "$tmp/profile.plist" "$2" "$3" <<'PY'
import plistlib, sys
p = plistlib.load(open(sys.argv[1], "rb"))
e, out_path, is_app = p["Entitlements"], sys.argv[2], sys.argv[3] == "app"
out = {k: e[k] for k in ("com.apple.application-identifier", "com.apple.developer.team-identifier") if k in e}
kinds = e.get("com.apple.developer.networking.networkextension", [])
kind = next((k for k in ("packet-tunnel-provider-systemextension", "packet-tunnel-provider") if k in kinds), None)
if kind is None:
    sys.exit(f"{sys.argv[1]}: no packet tunnel (Network Extensions capability)")
out["com.apple.developer.networking.networkextension"] = [kind]
groups = e.get("com.apple.security.application-groups", [])
if not groups:
    sys.exit(f"{sys.argv[1]}: no app group")
out["com.apple.security.application-groups"] = groups[:1]
if is_app:
    if not e.get("com.apple.developer.system-extension.install"):
        sys.exit(f"{sys.argv[1]}: no System Extension capability")
    out["com.apple.developer.system-extension.install"] = True
if e.get("com.apple.security.get-task-allow"):
    out["com.apple.security.get-task-allow"] = True
plistlib.dump(out, open(out_path, "wb"))
print(groups[0])
PY
}
group=$(entitlements "$ext_profile" "$tmp/ext.entitlements" ext)
entitlements "$app_profile" "$tmp/app.entitlements" app >/dev/null

# 5. The bundle.
app="$work/$name.app"
contents="$app/Contents"
sysext="$contents/Library/SystemExtensions/$ext_id.systemextension"
rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Resources" "$sysext/Contents/MacOS"
cp "$work/$crate" "$contents/MacOS/$crate"
cp -R "$root/assets" "$contents/Resources/assets"
# DrawnUI reads the app's files next to the executable; the files themselves stay in Resources.
ln -s ../Resources/assets "$contents/MacOS/assets"
cp "$work/$ext_id" "$sysext/Contents/MacOS/$ext_id"

# The icon from icon.ico: its largest image at every macOS icon size.
iconset="$work/AppIcon.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
sips -s format png "$root/icon.ico" --out "$work/icon-src.png" >/dev/null
for size in 16 32 128 256 512; do
    sips -z $size $size "$work/icon-src.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "$work/icon-src.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$contents/Resources/AppIcon.icns"
rm -rf "$iconset" "$work/icon-src.png"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
build_number=$(git rev-list --count HEAD 2>/dev/null || echo 1)

cat > "$contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$crate</string>
    <key>CFBundleIdentifier</key><string>$bundle_id</string>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>$build_number</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleLocalizations</key><array><string>en</string><string>ru</string></array>
    <key>LSMinimumSystemVersion</key><string>$min_macos</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>EngineVersion</key><string>$engine_version</string>
</dict>
</plist>
PLIST

cat > "$sysext/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$ext_id</string>
    <key>CFBundleIdentifier</key><string>$ext_id</string>
    <key>CFBundleName</key><string>Connection</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundlePackageType</key><string>SYSX</string>
    <key>CFBundleVersion</key><string>$build_number</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>$min_macos</string>
    <key>NSSystemExtensionUsageDescription</key><string>Connects this Mac through your server.</string>
    <key>NetworkExtension</key>
    <dict>
        <key>NEMachServiceName</key><string>$group.connection</string>
        <key>NEProviderClasses</key>
        <dict><key>com.apple.networkextension.packet-tunnel</key><string>Connection.Provider</string></dict>
    </dict>
</dict>
</plist>
PLIST

# 6. Signing: the extension first, then the app around it; the hardened runtime (needed for
# notarization) on both.
cp "$ext_profile" "$sysext/Contents/embedded.provisionprofile"
cp "$app_profile" "$contents/embedded.provisionprofile"
if [ "$action" = release ]; then stamp=--timestamp; else stamp=--timestamp=none; fi
codesign --force --options runtime $stamp --sign "$identity" --entitlements "$tmp/ext.entitlements" "$sysext"
codesign --force --options runtime $stamp --sign "$identity" --entitlements "$tmp/app.entitlements" "$app"
codesign --verify --deep --strict "$app"
echo "signed: $app"

if [ "$action" = release ]; then
    [ -n "${MAC_NOTARY_PROFILE:-}" ] || { echo "set MAC_NOTARY_PROFILE (xcrun notarytool store-credentials)"; exit 1; }
    zip="$work/netconnect-$(echo "$version" | tr -d .)-macos.zip"
    rm -f "$zip"
    ditto -c -k --keepParent "$app" "$zip"
    xcrun notarytool submit "$zip" --keychain-profile "$MAC_NOTARY_PROFILE" --wait
    xcrun stapler staple "$app"
    rm -f "$zip"
    ditto -c -k --keepParent "$app" "$zip"
    echo "release: $zip"
    exit 0
fi
[ "$action" = run ] || exit 0

# 7. Development: the extension can only be activated from /Applications. A running copy is quit
# first; the old bundle moves aside before the new one is copied in.
installed="/Applications/$name.app"
osascript -e "tell application id \"$bundle_id\" to quit" >/dev/null 2>&1 || true
if [ -d "$installed" ]; then
    old="$tmp/old.app"
    mv "$installed" "$old"
fi
ditto "$app" "$installed"
open "$installed"
echo "installed and opened: $installed"
