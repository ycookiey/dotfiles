$ErrorActionPreference = 'Stop'

if (Get-Command claude -ea 0) {
    Write-Host "Claude Code is already installed." -ForegroundColor Green
    return
}

Write-Host "Installing Claude Code..." -ForegroundColor Cyan
irm https://claude.ai/install.ps1 | iex
