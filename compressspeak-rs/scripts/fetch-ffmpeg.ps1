# Télécharge FFmpeg essentials Windows dans vendor/ffmpeg/windows/
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Dest = Join-Path $Root "vendor\ffmpeg\windows"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

$Url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"
$Zip = Join-Path $env:TEMP "ffmpeg-essentials.zip"
Write-Host "Téléchargement $Url ..."
Invoke-WebRequest -Uri $Url -OutFile $Zip

$Extract = Join-Path $env:TEMP "ffmpeg-extract"
if (Test-Path $Extract) { Remove-Item -Recurse -Force $Extract }
Expand-Archive -Path $Zip -DestinationPath $Extract -Force

$Bin = Get-ChildItem -Path $Extract -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
if (-not $Bin) { throw "ffmpeg.exe introuvable dans l'archive" }
Copy-Item $Bin.FullName (Join-Path $Dest "ffmpeg.exe") -Force

$Probe = Get-ChildItem -Path $Extract -Recurse -Filter "ffprobe.exe" | Select-Object -First 1
if ($Probe) {
    Copy-Item $Probe.FullName (Join-Path $Dest "ffprobe.exe") -Force
}

Write-Host "OK → $Dest"
