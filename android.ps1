# Android: builds libdrawnnetc.so with cargo-ndk, packs it with Gradle (android\, GameActivity) and,
# with -Install, installs and starts it on the phone or emulator adb sees.
#   .\android.ps1                         -> arm64-v8a (phones): target\android\drawnnetc.apk
#   .\android.ps1 -Abi armeabi-v7a        -> 32-bit ARM phones
#   .\android.ps1 -Abi x86_64 -Install    -> the emulator
#   .\android.ps1 -Release                -> the release files, short names people can type, for
#                                            version 0.1.0: netconnect-010-arm64.apk,
#                                            netconnect-010-armv7.apk and netconnect-010.aab (both)
# Needs: the Android SDK (ANDROID_HOME) with platform 36, NDK 26, a JDK 17 or newer (JAVA_HOME,
# Android Studio's jbr works), cargo-ndk (cargo install cargo-ndk) and the Rust Android targets.
param(
    [ValidateSet("arm64-v8a", "armeabi-v7a", "x86_64", "x86")]
    [string]$Abi = "arm64-v8a",
    [switch]$Install,
    [switch]$Release
)
$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    # NDK 26: the prebuilt Skia is built with it, and the app links Skia's C++ library from it.
    # ANDROID_NDK_HOME is used when it is a 26, else the SDK's NDK 26.
    if (-not $env:ANDROID_NDK_HOME -or (Split-Path -Leaf $env:ANDROID_NDK_HOME) -notlike "26.*") {
        $ndk = Get-ChildItem (Join-Path $env:ANDROID_HOME "ndk") -Directory -ErrorAction SilentlyContinue | Where-Object Name -like "26.*" | Select-Object -Last 1
        if (-not $ndk) { throw "NDK 26 not found: install it with the SDK Manager (ndk;26.1.10909125)" }
        $env:ANDROID_NDK_HOME = $ndk.FullName
    }
    if (-not $env:JAVA_HOME) { $env:JAVA_HOME = Join-Path $env:ProgramFiles "Android\Android Studio\jbr" }

    # Xray-core for Android (gomobile), a pinned release of AndroidLibXrayLite.
    if (-not (Test-Path android\libs\libv2ray.aar)) {
        New-Item -ItemType Directory -Force android\libs | Out-Null
        Invoke-WebRequest https://github.com/2dust/AndroidLibXrayLite/releases/download/v26.11.9/libv2ray.aar -OutFile android\libs\libv2ray.aar
    }
    # The app reads its files as "assets/...": they go to the APK as assets/assets/...
    Remove-Item -Recurse -Force target\android\stage -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force target\android\stage | Out-Null
    Copy-Item -Recurse assets target\android\stage\assets

    # Builds the native library for these ABIs (only these go into the package) and packs it with
    # Gradle: task assembleRelease (APK) or bundleRelease (AAB). Returns Gradle's output file.
    function Build-Package([string[]]$abis, [string]$task) {
        Remove-Item -Recurse -Force target\android\lib -ErrorAction SilentlyContinue
        $targets = $abis | ForEach-Object { "-t"; $_ }
        cargo ndk @targets --platform 26 -o target\android\lib rustc --lib --release --crate-type cdylib
        if ($LASTEXITCODE) { throw "cargo ndk failed" }
        & android\gradlew.bat -p android --project-cache-dir "$PSScriptRoot\target\android\gradle-cache" -q $task "-Pabis=$($abis -join ',')"
        if ($LASTEXITCODE) { throw "gradle $task failed" }
        if ($task -eq "bundleRelease") { "target\android\gradle\outputs\bundle\release\drawnnetc-release.aab" }
        else { "target\android\gradle\outputs\apk\release\drawnnetc-release.apk" }
    }

    if ($Release) {
        $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
        $short = "target\android\netconnect-$($version -replace '\.', '')"
        # armv7 first: the arm64 APK is also left as drawnnetc.apk for -Install.
        foreach ($pair in @(("armeabi-v7a", "armv7"), ("arm64-v8a", "arm64"))) {
            $apk = Build-Package @($pair[0]) assembleRelease
            Copy-Item $apk "$short-$($pair[1]).apk" -Force
            Write-Host "APK: $PSScriptRoot\$short-$($pair[1]).apk"
        }
        Copy-Item "$short-arm64.apk" target\android\drawnnetc.apk -Force
        $aab = Build-Package @("arm64-v8a", "armeabi-v7a") bundleRelease
        Copy-Item $aab "$short.aab" -Force
        Write-Host "AAB: $PSScriptRoot\$short.aab"
    } else {
        $apk = Build-Package @($Abi) assembleRelease
        Copy-Item $apk target\android\drawnnetc.apk -Force
        Write-Host "APK: $PSScriptRoot\target\android\drawnnetc.apk"
    }

    if ($Install) {
        adb install -r target\android\drawnnetc.apk
        if ($LASTEXITCODE) { throw "adb install failed" }
        adb shell am start -n com.appomobi.drawnnetc/.MainActivity
    }
}
finally {
    Pop-Location
}
