#!/bin/sh
# iPhone, on a Mac with Xcode and Go 1.27+: builds the app (Rust, DrawnUI) and its connection
# extension (Swift + the engine: Xray-core as a Go C archive), bundles them, signs both and starts
# the app on the iPhone.
#   ios/ios.sh            build, sign, install, launch
#   ios/ios.sh build      build and sign only (target/.../ios/Net Connect bundle)
#
# Signing, from the environment or from ios/signing.properties (KEY=value lines, not in git):
#   IOS_DEVICE        devicectl id or UDID; default: the connected iPhone (more than one: set it)
#   IOS_IDENTITY      codesigning identity; default: the first "Apple Development" one
#   IOS_APP_PROFILE   development profile for com.appomobi.drawnnetc
#   IOS_EXT_PROFILE   development profile for com.appomobi.drawnnetc.connection
# Both profiles need the Network Extensions capability (packet-tunnel-provider); without a path the
# newest installed development profile for that exact id and this iPhone is used.
set -eu
cd "$(dirname "$0")/.."
root="$PWD"

crate=drawnnetc
name="Net Connect"
bundle_id=com.appomobi.drawnnetc
ext_id=$bundle_id.connection
ext_name=Connection
launch_color=0B1220
min_ios=15.0

action=run
for arg in "$@"; do
    case "$arg" in
        build) action=build ;;
        run) action=run ;;
        *) echo "unknown option $arg"; exit 1 ;;
    esac
done

if [ -f ios/signing.properties ]; then
    while IFS='=' read -r key value; do
        case "$key" in
            IOS_DEVICE|IOS_IDENTITY|IOS_APP_PROFILE|IOS_EXT_PROFILE)
                eval "current=\${$key:-}"
                [ -n "$current" ] || export "$key=$value" ;;
        esac
    done < ios/signing.properties
fi

target=aarch64-apple-ios
sdk_path=$(xcrun --sdk iphoneos --show-sdk-path)
target_dir=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
out="$target_dir/$target/release"
work="$out/ios"
mkdir -p "$work"

# 1. The app: Rust (glue.m is compiled by build.rs).
cargo build --target "$target" --release

# 2. The engine: Xray-core with the iOS fd handover, a C archive for the extension.
engine_version=$(sed -n 's/.*Xray-core v\([0-9.]*\).*/\1/p' ios/engine/go.mod | head -1)
(
    cd ios/engine
    clang=$(xcrun --sdk iphoneos -f clang)
    flags="-isysroot $sdk_path -arch arm64 -miphoneos-version-min=$min_ios"
    GOOS=ios GOARCH=arm64 CGO_ENABLED=1 CC="$clang" CGO_CFLAGS="$flags" CGO_LDFLAGS="$flags" \
        go build -buildmode=c-archive -trimpath -ldflags="-s -w -buildid=" -o "$work/libengine.a" .
)

# 3. The extension: Swift, linked with the engine; its entry point is the system's extension main.
xcrun --sdk iphoneos swiftc -O -target "arm64-apple-ios$min_ios" -sdk "$sdk_path" \
    -module-name "$ext_name" -parse-as-library -application-extension \
    -import-objc-header ios/connection/Engine.h ios/connection/Provider.swift "$work/libengine.a" \
    -framework NetworkExtension -framework Security -framework CoreFoundation -lresolv \
    -Xlinker -e -Xlinker _NSExtensionMain -o "$work/$ext_name"

# 4. The bundle.
app="$work/$name.app"
appex="$app/PlugIns/$ext_name.appex"
rm -rf "$app"
mkdir -p "$appex"
cp "$out/$crate" "$app/$crate"
cp -R "$root/assets" "$app/assets"
cp "$work/$ext_name" "$appex/$ext_name"

icon_src="$work/icon-src.png"
sips -s format png "$root/icon.ico" --out "$icon_src" >/dev/null
sips -z 120 120 "$icon_src" --out "$app/AppIcon60x60@2x.png" >/dev/null
sips -z 180 180 "$icon_src" --out "$app/AppIcon60x60@3x.png" >/dev/null
rm -f "$icon_src"

assets="$work/assets.xcassets"
rm -rf "$assets"
mkdir -p "$assets/LaunchBackground.colorset"
printf '{"info":{"version":1,"author":"xcode"}}' > "$assets/Contents.json"
cat > "$assets/LaunchBackground.colorset/Contents.json" <<JSON
{"info":{"version":1,"author":"xcode"},"colors":[{"idiom":"universal","color":{"color-space":"srgb","components":{"red":"0x$(echo "$launch_color" | cut -c1-2)","green":"0x$(echo "$launch_color" | cut -c3-4)","blue":"0x$(echo "$launch_color" | cut -c5-6)","alpha":"1"}}}]}
JSON
xcrun actool --compile "$app" --platform iphoneos --minimum-deployment-target "$min_ios" \
    --target-device iphone --output-format human-readable-text \
    --output-partial-info-plist "$work/assets.plist" "$assets" >/dev/null
rm -rf "$assets" "$work/assets.plist"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
build_number=$(git rev-list --count HEAD 2>/dev/null || echo 1)

cat > "$app/Info.plist" <<PLIST
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
    <key>CFBundleSupportedPlatforms</key><array><string>iPhoneOS</string></array>
    <key>CFBundleLocalizations</key><array><string>en</string><string>ru</string></array>
    <key>LSRequiresIPhoneOS</key><true/>
    <key>MinimumOSVersion</key><string>$min_ios</string>
    <key>UIDeviceFamily</key><array><integer>1</integer></array>
    <key>UILaunchScreen</key><dict><key>UIColorName</key><string>LaunchBackground</string></dict>
    <key>UIRequiresFullScreen</key><true/>
    <key>CADisableMinimumFrameDurationOnPhone</key><true/>
    <key>UISupportedInterfaceOrientations</key><array><string>UIInterfaceOrientationPortrait</string></array>
    <key>CFBundleIcons</key>
    <dict>
        <key>CFBundlePrimaryIcon</key>
        <dict><key>CFBundleIconFiles</key><array><string>AppIcon60x60</string></array></dict>
    </dict>
    <key>EngineVersion</key><string>$engine_version</string>
</dict>
</plist>
PLIST

cat > "$appex/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$ext_name</string>
    <key>CFBundleIdentifier</key><string>$ext_id</string>
    <key>CFBundleName</key><string>$ext_name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundlePackageType</key><string>XPC!</string>
    <key>CFBundleVersion</key><string>$build_number</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleSupportedPlatforms</key><array><string>iPhoneOS</string></array>
    <key>MinimumOSVersion</key><string>$min_ios</string>
    <key>NSExtension</key>
    <dict>
        <key>NSExtensionPointIdentifier</key><string>com.apple.networkextension.packet-tunnel</string>
        <key>NSExtensionPrincipalClass</key><string>$ext_name.Provider</string>
    </dict>
</dict>
</plist>
PLIST

# 5. Signing: the extension first, then the app around it.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

xcrun devicectl list devices --json-output "$tmp/devices.json" >/dev/null
found=$(python3 - "$tmp/devices.json" "${IOS_DEVICE:-}" <<'PY'
import json, sys
wanted, found = sys.argv[2], []
for d in json.load(open(sys.argv[1]))["result"]["devices"]:
    hw, props = d.get("hardwareProperties", {}), d.get("connectionProperties", {})
    if hw.get("deviceType") != "iPhone":
        continue
    if wanted and wanted not in (d.get("identifier"), hw.get("udid")):
        continue
    if not wanted and props.get("tunnelState") in ("disconnected", "unavailable"):
        continue
    found.append((d["identifier"], hw.get("udid")))
if len(found) > 1:
    print("more than one iPhone; set IOS_DEVICE to one of:", *[f[0] for f in found], file=sys.stderr)
    sys.exit(1)
if found:
    print(*found[0])
PY
) || exit 1
set -- $found
[ $# -ge 2 ] || { echo "no iPhone found (IOS_DEVICE=${IOS_DEVICE:-})"; exit 1; }
device=$1 udid=$2

identity="${IOS_IDENTITY:-$(security find-identity -v -p codesigning | grep -m1 'Apple Development' | sed -E 's/.*"(.*)"/\1/')}"
[ -n "$identity" ] || { echo "no Apple Development identity"; exit 1; }

# The newest development profile for exactly this id, listing this iPhone, not expired.
find_profile() {
    python3 - "$udid" "$1" <<'PY'
import glob, os, plistlib, subprocess, sys, datetime
udid, bundle_id, best = sys.argv[1], sys.argv[2], None
folders = ["~/Library/Developer/Xcode/UserData/Provisioning Profiles", "~/Library/MobileDevice/Provisioning Profiles"]
for path in (p for f in folders for p in glob.glob(os.path.join(os.path.expanduser(f), "*.mobileprovision"))):
    try:
        p = plistlib.loads(subprocess.run(["security", "cms", "-D", "-i", path], capture_output=True).stdout)
    except Exception:
        continue
    e, team = p.get("Entitlements", {}), (p.get("TeamIdentifier") or [""])[0]
    if not e.get("get-task-allow") or udid not in p.get("ProvisionedDevices", []):
        continue
    if e.get("application-identifier") != f"{team}.{bundle_id}" or p["ExpirationDate"] < datetime.datetime.now():
        continue
    if best is None or p["CreationDate"] > best[0]:
        best = (p["CreationDate"], path)
print(best[1] if best else "")
PY
}
app_profile="${IOS_APP_PROFILE:-$(find_profile "$bundle_id")}"
ext_profile="${IOS_EXT_PROFILE:-$(find_profile "$ext_id")}"
[ -n "$app_profile" ] || { echo "no development profile for $bundle_id with this iPhone ($udid)"; exit 1; }
[ -n "$ext_profile" ] || { echo "no development profile for $ext_id with this iPhone ($udid)"; exit 1; }

# Entitlements: the ones the profile grants that the app needs.
entitlements() {
    security cms -D -i "$1" > "$tmp/profile.plist"
    python3 - "$tmp/profile.plist" "$2" <<'PY'
import plistlib, sys
p = plistlib.load(open(sys.argv[1], "rb"))
e = p["Entitlements"]
out = {k: e[k] for k in ("application-identifier", "com.apple.developer.team-identifier", "get-task-allow") if k in e}
if "packet-tunnel-provider" not in e.get("com.apple.developer.networking.networkextension", []):
    sys.exit(f"{sys.argv[1]}: no packet-tunnel-provider (Network Extensions capability)")
out["com.apple.developer.networking.networkextension"] = ["packet-tunnel-provider"]
if e.get("com.apple.security.application-groups"):
    out["com.apple.security.application-groups"] = e["com.apple.security.application-groups"]
plistlib.dump(out, open(sys.argv[2], "wb"))
PY
}
entitlements "$ext_profile" "$tmp/ext.entitlements"
entitlements "$app_profile" "$tmp/app.entitlements"
cp "$ext_profile" "$appex/embedded.mobileprovision"
cp "$app_profile" "$app/embedded.mobileprovision"
codesign --force --sign "$identity" --entitlements "$tmp/ext.entitlements" --timestamp=none "$appex"
codesign --force --sign "$identity" --entitlements "$tmp/app.entitlements" --timestamp=none "$app"
echo "signed: $app"

[ "$action" = run ] || exit 0
xcrun devicectl device install app --device "$device" "$app"
xcrun devicectl device process launch --device "$device" --terminate-existing "$bundle_id"
