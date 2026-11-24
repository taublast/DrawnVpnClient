# Windows: builds Net Connect (x64) and its folder: NetConnect.exe, assets\ and engine\ (Xray-core
# with the Wintun driver, a pinned release, checked against its published SHA-256).
#   .\windows.ps1           -> target\windows\Net Connect\ (and starts it with -Run)
#   .\windows.ps1 -Release  -> also target\windows\netconnect-<version without dots>-windows-x64.zip
# Connect asks for administrator rights (the Windows prompt): the tunnel runs as a second copy of the
# program, started as administrator, and stops on Disconnect or when the app closes.
param(
    [switch]$Release,
    [switch]$Run
)
$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    $xrayVersion = "v26.11.9"
    $xraySha256 = "1262522266337b824f07beb8e1375786b437683b3890105469088f8e17e0a69d"

    cargo build --release
    if ($LASTEXITCODE) { throw "cargo build failed" }

    $out = "target\windows\Net Connect"
    Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$out\engine" | Out-Null
    Copy-Item target\release\drawnnetc.exe "$out\NetConnect.exe"
    Copy-Item -Recurse assets "$out\assets"

    # The engine: Xray-core for Windows x64, with wintun.dll next to xray.exe.
    $zip = "target\windows\Xray-windows-64-$xrayVersion.zip"
    if (-not (Test-Path $zip)) {
        Invoke-WebRequest "https://github.com/XTLS/Xray-core/releases/download/$xrayVersion/Xray-windows-64.zip" -OutFile $zip
    }
    if ((Get-FileHash $zip -Algorithm SHA256).Hash.ToLower() -ne $xraySha256) { throw "Xray download does not match its SHA-256" }
    $unpacked = "target\windows\xray-$xrayVersion"
    Remove-Item -Recurse -Force $unpacked -ErrorAction SilentlyContinue
    Expand-Archive $zip $unpacked
    foreach ($file in "xray.exe", "wintun.dll", "LICENSE", "LICENSE-Wintun") {
        Copy-Item "$unpacked\$file" "$out\engine\$file"
    }
    Write-Host "App: $PSScriptRoot\$out\NetConnect.exe"

    if ($Release) {
        $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
        $file = "target\windows\netconnect-$($version -replace '\.', '')-windows-x64.zip"
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
