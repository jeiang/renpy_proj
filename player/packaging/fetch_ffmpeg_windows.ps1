# Fetches the LGPL FFmpeg 7.1 shared build for Windows x64 (BtbN FFmpeg-Builds) into player\upstream\ffmpeg-win.
# Never vendored in git. The archive is checked by SHA-256, then `ffmpeg -L` and the build configuration are
# checked: the build must be LGPL (no GPL, no nonfree). Result: upstream\ffmpeg-win\{bin,include,lib}; the
# player build uses it through FFMPEG_DIR, and packaging\windows.ps1 copies bin\*.dll beside player.exe.
#
#   powershell -NoProfile -File packaging\fetch_ffmpeg_windows.ps1
$ErrorActionPreference = 'Stop'
$Tag  = 'autobuild-2026-07-31-14-10'
$Name = 'ffmpeg-n7.1.5-12-g1fdbca85aa-win64-lgpl-shared-7.1.zip'
$Sha  = '0f376f96fb38554ccefb1b2ae9c7c6a7b351f0e60a372b38262c320e8392c5d0'
$Url  = "https://github.com/BtbN/FFmpeg-Builds/releases/download/$Tag/$Name"

$player = Split-Path -Parent $PSScriptRoot
$up     = Join-Path $player 'upstream'
$dest   = Join-Path $up 'ffmpeg-win'
$stamp  = Join-Path $dest '.sha256'
if ((Test-Path $stamp) -and ((Get-Content $stamp) -eq $Sha)) { Write-Host "ffmpeg-win ready: $dest"; exit 0 }

New-Item -ItemType Directory -Force $up | Out-Null
$zip = Join-Path $up $Name
if (-not (Test-Path $zip)) {
    Write-Host "fetching $Url"
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri $Url -OutFile $zip -UseBasicParsing
}
$got = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
if ($got -ne $Sha) { Remove-Item $zip; throw "checksum mismatch for ${Name}: $got" }

$tmp = Join-Path $up 'ffmpeg-win.tmp'
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
Expand-Archive -Path $zip -DestinationPath $tmp
$root = Get-ChildItem $tmp | Select-Object -First 1
if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
Move-Item $root.FullName $dest
Remove-Item -Recurse -Force $tmp

$ff = Join-Path $dest 'bin\ffmpeg.exe'
$ErrorActionPreference = 'Continue'   # ffmpeg writes its banner to stderr
$lic = (cmd /c "`"$ff`" -hide_banner -L 2>&1") -join "`n"
$cfg = (cmd /c "`"$ff`" -hide_banner -buildconf 2>&1") -join "`n"
$ErrorActionPreference = 'Stop'
if ($lic -match 'GNU General Public License' -and $lic -notmatch 'Lesser') { throw "ffmpeg -L reports a GPL build" }
if ($lic -notmatch 'GNU Lesser General Public License') { throw "ffmpeg -L does not report the LGPL: $lic" }
if ($cfg -match '--enable-gpl|--enable-nonfree') { throw "ffmpeg build configuration has GPL or nonfree options" }
Set-Content -Path $stamp -Value $Sha
Write-Host "ffmpeg-win ready: $dest"
Write-Host ($lic.Trim())
