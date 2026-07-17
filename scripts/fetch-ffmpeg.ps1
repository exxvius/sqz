<#
.SYNOPSIS
  Fetch a full (GPL) FFmpeg build and place ffmpeg/ffprobe as Tauri sidecars for
  Windows, named with the Rust target triple.

.EXAMPLE
  ./scripts/fetch-ffmpeg.ps1
  ./scripts/fetch-ffmpeg.ps1 -Triple x86_64-pc-windows-msvc
#>
param(
  [string]$Triple = (& rustc -Vv | Select-String '^host: ' | ForEach-Object { $_.Line -replace '^host: ', '' }),
  [string]$Url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
)

$ErrorActionPreference = "Stop"
$binDir = Join-Path $PSScriptRoot "..\src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

if ([string]::IsNullOrWhiteSpace($Triple)) {
  throw "Could not determine target triple (is rustc installed?)."
}
Write-Host "Target triple: $Triple"

$tmp = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ("ff_" + [guid]::NewGuid()))
try {
  $zip = Join-Path $tmp "ff.zip"
  Write-Host "Downloading $Url"
  Invoke-WebRequest -Uri $Url -OutFile $zip
  Expand-Archive -Path $zip -DestinationPath $tmp -Force

  $ffmpeg = Get-ChildItem -Path $tmp -Recurse -Filter ffmpeg.exe | Select-Object -First 1
  $ffprobe = Join-Path $ffmpeg.DirectoryName "ffprobe.exe"

  Copy-Item $ffmpeg.FullName (Join-Path $binDir "ffmpeg-$Triple.exe") -Force
  Copy-Item $ffprobe (Join-Path $binDir "ffprobe-$Triple.exe") -Force
  Write-Host "  → ffmpeg-$Triple.exe"
  Write-Host "  → ffprobe-$Triple.exe"
  Write-Host "Done. Sidecars are in src-tauri/binaries/ (git-ignored)."
}
finally {
  Remove-Item -Recurse -Force $tmp
}
