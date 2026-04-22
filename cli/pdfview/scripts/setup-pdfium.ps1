# Downloads pdfium.dll from pdfium-binaries and drops it next to the
# compiled pdfview binary. Development-time convenience; release
# installs get the DLL via the yscoopy Scoop manifest instead.
#
# Usage:
#     .\scripts\setup-pdfium.ps1                 # default: target\debug
#     .\scripts\setup-pdfium.ps1 -Profile release
#     .\scripts\setup-pdfium.ps1 -Release chromium/7802

[CmdletBinding()]
param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',
    [string]$Release = 'chromium/7802'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$crateRoot = Split-Path -Parent $scriptDir
$workspaceRoot = Split-Path -Parent $crateRoot
$targetDir = Join-Path $workspaceRoot "target\$Profile"

if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
}

$dll = Join-Path $targetDir 'pdfium.dll'
if (Test-Path $dll) {
    Write-Host "pdfium.dll already present at $dll - skipping"
    exit 0
}

$url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$Release/pdfium-win-x64.tgz"
$tmp = New-TemporaryFile
$tgz = "$($tmp.FullName).tgz"
Remove-Item $tmp

Write-Host "Downloading $url"
Invoke-WebRequest -Uri $url -OutFile $tgz

$extractDir = Join-Path ([System.IO.Path]::GetTempPath()) ("pdfium-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $extractDir | Out-Null

Write-Host "Extracting to $extractDir"
tar -xzf $tgz -C $extractDir
if ($LASTEXITCODE -ne 0) {
    throw "tar extract failed with exit code $LASTEXITCODE"
}

$source = Join-Path $extractDir 'bin\pdfium.dll'
if (-not (Test-Path $source)) {
    throw "pdfium.dll not found in archive at $source"
}

Copy-Item $source $dll
Write-Host "Installed pdfium.dll -> $dll"

Remove-Item $tgz -Force
Remove-Item $extractDir -Recurse -Force
