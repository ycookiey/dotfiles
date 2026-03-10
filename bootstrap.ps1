# bootstrap.ps1 — Fresh PC: irm https://raw.githubusercontent.com/ycookiey/dotfiles/main/bootstrap.ps1 | iex
$ErrorActionPreference = "Stop"
$Repo = "https://github.com/ycookiey/dotfiles.git"
$Dir  = "C:\Main\Project\dotfiles"

Write-Host "=== dotfiles bootstrap ===" -ForegroundColor Cyan

# Scoop
if (!(Get-Command scoop -ea 0)) {
    Set-ExecutionPolicy RemoteSigned -Scope CurrentUser -Force
    irm get.scoop.sh | iex
}

# git + pwsh（clone & setup.ps1 に必要な最小セット）
'git', 'pwsh' | % { if (!(Get-Command $_ -ea 0)) { scoop install $_ } }

# Clone
if (!(Test-Path $Dir)) {
    [void](New-Item (Split-Path $Dir) -ItemType Directory -Force)
    git clone $Repo $Dir
}

# Setup（--scoop で全アプリも一括インストール）
& "$HOME\scoop\shims\pwsh.exe" -ExecutionPolicy Bypass -File "$Dir\setup.ps1" --scoop

Write-Host "`nDone! Restart terminal to apply." -ForegroundColor Green
