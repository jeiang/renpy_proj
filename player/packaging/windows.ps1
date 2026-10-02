# Windows x64 package: player.exe (static CPython, /MT) plus the LGPL FFmpeg DLLs beside it.
#
#   powershell -NoProfile -File packaging\windows.ps1 [-NoBuild]
#
# Output (gitignored): player\build-out\windows\player-windows-x86_64\{player.exe, avcodec-*.dll, ...}
# and player-windows-x86_64.zip next to it.
#
# Steps
#   1. fetch_ffmpeg_windows.ps1: the BtbN LGPL 7.1 shared build (SHA-256 pinned, `ffmpeg -L` checked).
#   2. `cargo build --release -p player` in a Visual Studio x64 shell (found with vswhere when `cl` is not
#      on PATH). Needs Python 3.12 with Cython 3.x (`python` on PATH or $env:PYTHON), Git for Windows,
#      rustup (stable-x86_64-pc-windows-msvc) and, for the static libffi and OpenSSL, vcpkg
#      ($env:PLAYER_VCPKG, default C:\spike\vcpkg). bindgen (ffmpeg-sys-next) needs libclang.dll:
#      $env:LIBCLANG_PATH, or the `libclang` pip wheel in the Python above.
#   3. Copies player.exe and the FFmpeg DLLs. Checks with dumpbin /dependents that the exe imports only
#      Windows system DLLs and the FFmpeg DLLs (no VCRUNTIME140.dll, no api-ms-win-crt-*).
#   4. Signing is the last step and is not done here (needs a real certificate): do not append data to the
#      signed exe. See research/win-spike section 5.2.
param([switch]$NoBuild)
$ErrorActionPreference = 'Stop'

$player = Split-Path -Parent $PSScriptRoot
$ff     = Join-Path $player 'upstream\ffmpeg-win'
$out    = Join-Path $player 'build-out\windows'
$pkg    = Join-Path $out 'player-windows-x86_64'

& powershell -NoProfile -File (Join-Path $PSScriptRoot 'fetch_ffmpeg_windows.ps1')
if ($LASTEXITCODE) { throw 'fetch_ffmpeg_windows.ps1 failed' }

# VS x64 environment.
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $vs = & $vswhere -latest -products '*' -property installationPath
    if (-not $vs) { throw 'Visual Studio 2022 Build Tools not found' }
    cmd /c "`"$vs\VC\Auxiliary\Build\vcvars64.bat`" >nul && set" | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] }
    }
}
$env:PYTHONUTF8 = '1'
$env:FFMPEG_DIR = $ff
if (-not $env:PYTHON) { $env:PYTHON = (Get-Command python.exe).Source }

if (-not $NoBuild) {
    if (-not $env:LIBCLANG_PATH) {
        $lc = & $env:PYTHON -c "import clang.native as n, os; print(os.path.dirname(n.__file__))" 2>$null
        if ($lc) { $env:LIBCLANG_PATH = $lc }
    }
    Push-Location $player
    try {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        cargo build --release -p player
        if ($LASTEXITCODE) { throw 'cargo build failed' }
        Write-Host ("build time: {0:n0} s" -f $sw.Elapsed.TotalSeconds)
    } finally { Pop-Location }
}

$exe = Join-Path $player 'target\release\player.exe'
if (-not (Test-Path $exe)) { throw "missing $exe" }
if (Test-Path $pkg) { Remove-Item -Recurse -Force $pkg }
New-Item -ItemType Directory -Force $pkg | Out-Null
Copy-Item $exe $pkg
Copy-Item (Join-Path $ff 'bin\*.dll') $pkg

# Licence notices (same set as packaging/licences/stage.sh) plus the licence files of the BtbN FFmpeg build.
$repo = Split-Path -Parent $player
$lic  = Join-Path $pkg 'licenses'
New-Item -ItemType Directory -Force $lic | Out-Null
foreach ($f in 'LICENSE-MIT', 'LICENSE-APACHE', 'THIRD_PARTY.md') {
    $src = Join-Path $repo $f
    if (-not (Test-Path $src)) { throw "missing $src" }
    Copy-Item $src $lic
}
$notices = Join-Path $PSScriptRoot 'licences'
Copy-Item (Join-Path $notices '*.txt') $lic
Copy-Item (Join-Path $notices 'RUST_DEPENDENCIES.md') $lic
$ffLic = Join-Path $ff 'LICENSE.txt'
if (Test-Path $ffLic) { Copy-Item $ffLic (Join-Path $lic 'FFmpeg-BtbN-LICENSE.txt') }

# Imports: system DLLs and FFmpeg only.
$deps = (cmd /c "dumpbin /dependents `"$pkg\player.exe`"") | Where-Object { $_ -match '^\s+\S+\.dll\s*$' } | ForEach-Object { $_.Trim() }
$bad = $deps | Where-Object { $_ -match '^(vcruntime|msvcp|api-ms-win-crt|ucrtbase)' }
Write-Host "player.exe imports: $($deps -join ' ')"
if ($bad) { throw "player.exe depends on the dynamic CRT: $($bad -join ' ')" }
Write-Host ("player.exe size: {0:n1} MB" -f ((Get-Item "$pkg\player.exe").Length / 1MB))

$zip = Join-Path $out 'player-windows-x86_64.zip'
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $pkg -DestinationPath $zip
Write-Host "package: $pkg and $zip"
