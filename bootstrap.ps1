# bootstrap.ps1 -- Fresh PC: irm https://raw.githubusercontent.com/ycookiey/dotfiles/main/bootstrap.ps1 | iex
$ErrorActionPreference = "Stop"
$env:SCOOP_ALLOW_ADMIN = "true"
$Repo = "https://github.com/ycookiey/dotfiles.git"
$Dir  = "C:\Main\Project\dotfiles"

Write-Host "=== dotfiles bootstrap ===" -ForegroundColor Cyan

# Check admin privileges
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

# Scoop
$scoopShims = $(if ($env:SCOOP) { "$env:SCOOP\shims" } else { "$HOME\scoop\shims" })
if (!(Get-Command scoop -ea 0)) {
    Set-ExecutionPolicy RemoteSigned -Scope CurrentUser -Force
    if ($isAdmin) {
        Write-Host "Running Scoop installer as Administrator..." -ForegroundColor Yellow
        iex "& {$(irm get.scoop.sh)} -RunAsAdmin"
    } else {
        irm get.scoop.sh | iex
    }
    # Add scoop shims to current session PATH (installer only updates registry)
    if ($scoopShims -notin ($env:PATH -split ';')) { $env:PATH = "$scoopShims;$env:PATH" }
}

# git + pwsh (minimum set for clone & setup.ps1)
'git', 'pwsh' | % { if (!(Get-Command $_ -ea 0)) { scoop install $_ } }

# Clone
if (!(Test-Path $Dir)) {
    [void](New-Item (Split-Path $Dir) -ItemType Directory -Force)
    git clone $Repo $Dir
}

# Phase 1: core apps (skip large -- abort on failure)
$pwsh = Join-Path $scoopShims 'pwsh.exe'
& $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\install\scoop.ps1" -SkipLarge
if ($LASTEXITCODE -ne 0) {
    throw "Phase 1 failed with exit code $LASTEXITCODE."
}

# Phase 2: config, symlinks, dotcli, startup (only needs core apps)
& $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\setup.ps1"
if ($LASTEXITCODE -ne 0) {
    throw "Phase 2 (setup) failed with exit code $LASTEXITCODE."
}

# Phase 3: mise install (dev tool runtimes)
if (Get-Command mise -ea 0) {
    Write-Host "`nInstalling mise tools..." -ForegroundColor Cyan
    mise install --yes
}

# Phase 4: large apps (non-fatal)
& $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\install\scoop.ps1" -OnlyLarge

Write-Host "`nDone! Restart terminal to apply." -ForegroundColor Green
