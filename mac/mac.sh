#!/bin/sh
# macOS, on a Mac with Xcode and Go 1.27+: builds the app (Rust, DrawnUI) and its connection system
# extension (Swift + the engine: Xray-core as a Go C archive), both universal (Apple silicon + Intel),
# signs them with Developer ID and notarizes them, and packs the release as a drag-to-install disk
# image: target/.../macos/netconnect-<version without dots>-macos.dmg.
#   mac/mac.sh            build, sign, notarize, make the DMG, install into /Applications and open
#   mac/mac.sh release    build, sign, notarize, make the DMG
#   mac/mac.sh build      build only, ad-hoc signed (for checking the build; it cannot connect)
# A system extension runs only from /Applications; the first connect asks the user to allow it in
# System Settings > Privacy & Security, then to allow the VPN configuration.
#
# Signing and notarizing go through the Apple ID that Xcode is signed in to (Xcode > Settings >
# Accounts, a member of the team): `xcodebuild -exportArchive` with Developer ID and automatic
# signing makes the profiles the tunnel and the system extension need, signs with the team's
# cloud-managed Developer ID certificate and submits for notarization. Nothing of the account is
# stored here. The team: MAC_TEAM from the environment or mac/signing.properties (not in git).
# The DMG itself is signed and notarized only when this Mac has the team's own "Developer ID
# Application" certificate and a notarytool keychain profile (MAC_NOTARY_PROFILE, made with
# `xcrun notarytool store-credentials`); otherwise it ships unsigned, which is fine: the app in it
# is signed, notarized and stapled, and that is what Gatekeeper checks.
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
            MAC_TEAM|MAC_NOTARY_PROFILE)
                eval "current=\${$key:-}"
                [ -n "$current" ] || export "$key=$value" ;;
        esac
    done < mac/signing.properties
fi
# The team is never guessed: a keychain can hold certificates of several teams, and the first
# one found may be another team's.
team="${MAC_TEAM:-}"
[ -n "$team" ] || { echo "set MAC_TEAM (the team to sign for) in the environment or mac/signing.properties"; exit 1; }
group="$team.$bundle_id"

sdk_path=$(xcrun --sdk macosx --show-sdk-path)
target_dir=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
work="$target_dir/macos"
mkdir -p "$work"
export MACOSX_DEPLOYMENT_TARGET=$min_macos

# 1. The app: Rust for both architectures (the glue is compiled by build.rs), one universal binary.
# The builder's paths are remapped, so the binary names no home folder.
RUSTFLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=$root=/build"
export RUSTFLAGS
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target add "$target" >/dev/null 2>&1 || true
    cargo build --target "$target" --release
done
unset RUSTFLAGS
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

# 4. The bundle. Nothing but code in MacOS/: the notary service reads everything there as code.
app="$work/$name.app"
contents="$app/Contents"
sysext="$contents/Library/SystemExtensions/$ext_id.systemextension"
rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Resources" "$sysext/Contents/MacOS"
cp "$work/$crate" "$contents/MacOS/$crate"
cp -R "$root/assets" "$contents/Resources/assets"
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

# 5. A first, ad-hoc signature with the hardened runtime and the entitlements the bundles need
# (the tunnel of a system extension, the app group its service name starts with, installing the
# extension): Xcode's export reads them from it, makes matching profiles and signs again.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cat > "$tmp/ext.entitlements" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.developer.networking.networkextension</key><array><string>packet-tunnel-provider-systemextension</string></array>
    <key>com.apple.security.application-groups</key><array><string>$group</string></array>
</dict>
</plist>
PLIST
cat > "$tmp/app.entitlements" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.developer.networking.networkextension</key><array><string>packet-tunnel-provider-systemextension</string></array>
    <key>com.apple.developer.system-extension.install</key><true/>
    <key>com.apple.security.application-groups</key><array><string>$group</string></array>
</dict>
</plist>
PLIST
codesign --force --options runtime --sign - --entitlements "$tmp/ext.entitlements" "$sysext"
codesign --force --options runtime --sign - --entitlements "$tmp/app.entitlements" "$app"
echo "built: $app"
[ "$action" = build ] && exit 0

# 6. Developer ID and notarization through Xcode's account: a hand-made archive is all
# `xcodebuild -exportArchive` needs.
archive="$work/NetConnect.xcarchive"
rm -rf "$archive" "$work/uploaded" "$work/notarized"
mkdir -p "$archive/Products/Applications"
cp -R "$app" "$archive/Products/Applications/"
cat > "$archive/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>ArchiveVersion</key><integer>2</integer>
    <key>Name</key><string>$name</string>
    <key>SchemeName</key><string>$name</string>
    <key>CreationDate</key><date>$(date -u +%Y-%m-%dT%H:%M:%SZ)</date>
    <key>ApplicationProperties</key><dict>
        <key>ApplicationPath</key><string>Applications/$name.app</string>
        <key>CFBundleIdentifier</key><string>$bundle_id</string>
        <key>CFBundleShortVersionString</key><string>$version</string>
        <key>CFBundleVersion</key><string>$build_number</string>
    </dict>
</dict>
</plist>
PLIST
cat > "$tmp/exportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>method</key><string>developer-id</string>
    <key>teamID</key><string>$team</string>
    <key>signingStyle</key><string>automatic</string>
    <key>destination</key><string>upload</string>
</dict>
</plist>
PLIST
echo "signing with Developer ID and submitting for notarization"
if ! xcodebuild -exportArchive -archivePath "$archive" -exportOptionsPlist "$tmp/exportOptions.plist" \
        -exportPath "$work/uploaded" -allowProvisioningUpdates > "$work/upload.log" 2>&1; then
    echo "could not sign through the Apple account; see $work/upload.log" >&2
    exit 1
fi
release=
echo "waiting for notarization"
for _ in $(seq 1 30); do
    sleep 20
    rm -rf "$work/notarized"
    if xcodebuild -exportNotarizedApp -archivePath "$archive" -exportPath "$work/notarized" > "$work/notarize.log" 2>&1; then
        release="$work/notarized/$name.app"
        break
    fi
    # "Invalid" is Apple's verdict, not a wait: stop and show why.
    if grep -q 'error: Invalid' "$work/notarize.log"; then
        echo "notarization was refused; Apple's reasons:" >&2
        for log in "$archive"/Submissions/*/notarization-log.json; do
            [ -f "$log" ] && python3 -c 'import json,sys
d = json.load(open(sys.argv[1]))
print(" ", d.get("status"), "-", d.get("statusSummary"))
for i in d.get("issues") or []:
    print("  -", i.get("severity"), i.get("path", ""), i.get("message"))' "$log" >&2
        done
        exit 1
    fi
done
[ -n "$release" ] || { echo "notarization did not finish in time; run again (Apple keeps working on it)" >&2; exit 1; }
xcrun stapler validate "$release" >/dev/null
spctl --assess --type execute "$release" >/dev/null
echo "notarized, stapled, and Gatekeeper accepts it"

# 7. The DMG: the app and a link to Applications to drag it onto.
dmg="$work/netconnect-$(echo "$version" | tr -d .)-macos.dmg"
rm -rf "$tmp/dmg" "$dmg"
mkdir -p "$tmp/dmg"
ditto "$release" "$tmp/dmg/$name.app"
ln -s /Applications "$tmp/dmg/Applications"
hdiutil create -quiet -volname "$name" -srcfolder "$tmp/dmg" -fs HFS+ -format UDZO -ov "$dmg"
identity=$(security find-identity -v -p codesigning \
    | sed -n "s/.*\"\(Developer ID Application: .*($team)\)\"/\1/p" | head -n 1)
profile="${MAC_NOTARY_PROFILE:-}"
if [ -n "$identity" ] && [ -n "$profile" ]; then
    if codesign --force --sign "$identity" --timestamp "$dmg" \
            && xcrun notarytool submit "$dmg" --keychain-profile "$profile" --wait > "$work/notarize-dmg.log" 2>&1 \
            && xcrun stapler staple "$dmg" >/dev/null; then
        echo "the DMG is signed, notarized and stapled too"
    else
        echo "note: signing or notarizing the DMG failed (see $work/notarize-dmg.log); the app in it is notarized"
    fi
else
    echo "note: the DMG itself is not signed (no Developer ID Application certificate of $team or no MAC_NOTARY_PROFILE); the app in it is signed and notarized"
fi
echo "release: $dmg"
[ "$action" = release ] && exit 0

# 8. Install: the extension can only be activated from /Applications. A running copy is quit
# first; the old bundle moves aside before the new one is copied in.
installed="/Applications/$name.app"
osascript -e "tell application id \"$bundle_id\" to quit" >/dev/null 2>&1 || true
if [ -d "$installed" ]; then
    mv "$installed" "$tmp/old.app"
fi
ditto "$release" "$installed"
open "$installed"
echo "installed and opened: $installed"
