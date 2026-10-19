# Downloads the latest libmpv build (shinchiro/mpv-winbuild-cmake) into ./libmpv.
# Requires 7-Zip (7z on PATH or in Program Files).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'libmpv'
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("libmpv-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Force $dest, $tmp | Out-Null

$sevenZip = (Get-Command 7z -ErrorAction SilentlyContinue).Source
if (-not $sevenZip -and (Test-Path 'C:\Program Files\7-Zip\7z.exe')) { $sevenZip = 'C:\Program Files\7-Zip\7z.exe' }
if (-not $sevenZip) { throw '7-Zip is required: winget install 7zip.7zip' }

$release = Invoke-RestMethod 'https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest'
$asset = $release.assets | Where-Object { $_.name -like 'mpv-dev-x86_64-2*.7z' } | Select-Object -First 1
if (-not $asset) { throw "No mpv-dev-x86_64 asset in release $($release.tag_name)" }

Write-Host "Downloading $($asset.name)..."
$archive = Join-Path $tmp $asset.name
Invoke-WebRequest $asset.browser_download_url -OutFile $archive
& $sevenZip e $archive "-o$dest" 'libmpv-2.dll' -y | Out-Null
Remove-Item -Recurse -Force $tmp
Write-Host "libmpv-2.dll -> $dest"
