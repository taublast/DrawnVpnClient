# Windows: builds Net Connect (x64 or ARM64) and its folder: NetConnect.exe, assets\ and engine\
# (Xray-core with the Wintun driver, a pinned release, checked against its published SHA-256).
#   .\windows.ps1                  -> target\windows\Net Connect\ (and starts it with -Run)
#   .\windows.ps1 -Release         -> also target\windows\netconnect-<version without dots>-windows-x64.zip
#   .\windows.ps1 -Arch arm64 ...  -> ARM64, cross-built here: target\windows-arm64\, -windows-arm64.zip
# Connect asks for administrator rights (the Windows prompt): the tunnel runs as a second copy of the
# program, started as administrator, and stops on Disconnect or when the app closes.
param(
    [ValidateSet("x64", "arm64")][string]$Arch = "x64",
    [switch]$Release,
    [switch]$Run
)
$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    $xrayVersion = "v26.11.9"
    if ($Arch -eq "arm64") {
        $target = "aarch64-pc-windows-msvc"; $xrayZip = "Xray-windows-arm64-v8a"; $dir = "target\windows-arm64"
        $xraySha256 = "9fdd48ff890e54ffdc37f92df9037c940fe4d4f914755b70cc644b97e9678a83"
    } else {
        $target = "x86_64-pc-windows-msvc"; $xrayZip = "Xray-windows-64"; $dir = "target\windows"
        $xraySha256 = "1262522266337b824f07beb8e1375786b437683b3890105469088f8e17e0a69d"
    }

    cargo build --release --target $target
    if ($LASTEXITCODE) { throw "cargo build failed" }

    $out = "$dir\Net Connect"
    Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$out\engine" | Out-Null
    Copy-Item "target\$target\release\drawnnetc.exe" "$out\NetConnect.exe"
    Copy-Item -Recurse assets "$out\assets"

    # The engine: Xray-core for this architecture, with wintun.dll next to xray.exe.
    $zip = "$dir\$xrayZip-$xrayVersion.zip"
    if (-not (Test-Path $zip)) {
        Invoke-WebRequest "https://github.com/XTLS/Xray-core/releases/download/$xrayVersion/$xrayZip.zip" -OutFile $zip
    }
    if ((Get-FileHash $zip -Algorithm SHA256).Hash.ToLower() -ne $xraySha256) { throw "Xray download does not match its SHA-256" }
    $unpacked = "$dir\xray-$xrayVersion"
    Remove-Item -Recurse -Force $unpacked -ErrorAction SilentlyContinue
    Expand-Archive $zip $unpacked
    foreach ($file in "xray.exe", "wintun.dll", "LICENSE", "LICENSE-Wintun") {
        Copy-Item "$unpacked\$file" "$out\engine\$file"
    }
    Write-Host "App: $PSScriptRoot\$out\NetConnect.exe"

    if ($Release) {
        $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
        $file = "$dir\netconnect-$($version -replace '\.', '')-windows-$Arch.zip"
        Remove-Item $file -ErrorAction SilentlyContinue
        Compress-Archive -Path $out -DestinationPath $file
        Write-Host "Release: $PSScriptRoot\$file"
    }
    if ($Run) {
        Start-Process "$out\NetConnect.exe"
    }
}
finally {
    Pop-Location
}
