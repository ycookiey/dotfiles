# bootstrap.ps1 -- Fresh PC: irm https://raw.githubusercontent.com/ycookiey/dotfiles/main/bootstrap.ps1 | iex
$ErrorActionPreference = "Stop"
$env:SCOOP_ALLOW_ADMIN = "true"
$Repo = "https://github.com/ycookiey/dotfiles.git"
$Dir  = "C:\Main\Project\dotfiles"

Write-Host "=== dotfiles bootstrap ===" -ForegroundColor Cyan

# Check admin privileges
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

# System locale UTF-8 (requires admin + reboot)
if ($isAdmin) {
    $nlsPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Nls\CodePage"
    $current = Get-ItemProperty $nlsPath
    if ($current.ACP -ne "65001") {
        Write-Host "Setting system locale to UTF-8..." -ForegroundColor Yellow
        Set-ItemProperty $nlsPath -Name ACP   -Value "65001"
        Set-ItemProperty $nlsPath -Name OEMCP -Value "65001"
        Set-ItemProperty $nlsPath -Name MACCP -Value "65001"
        $script:needsReboot = $true
    }

    # UAC prompt on desktop (not secure desktop)
    $uacPath = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System"
    $uac = Get-ItemProperty $uacPath
    if ($uac.PromptOnSecureDesktop -ne 0) {
        Write-Host "Setting UAC prompt to desktop..." -ForegroundColor Yellow
        Set-ItemProperty $uacPath -Name PromptOnSecureDesktop -Value 0
    }

    # Skip lock screen
    $personalizationPath = "HKLM:\SOFTWARE\Policies\Microsoft\Windows\Personalization"
    $personalization = Get-ItemProperty $personalizationPath -ea 0
    if ($personalization.NoLockScreen -ne 1) {
        Write-Host "Disabling lock screen..." -ForegroundColor Yellow
        Set-ItemProperty $personalizationPath -Name NoLockScreen -Value 1 -Force
    }
}

# HKCU settings
$advPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"
$adv = Get-ItemProperty $advPath -ea 0

# Show file extensions
if ($adv.HideFileExt -ne 0) {
    Write-Host "Showing file extensions..." -ForegroundColor Yellow
    Set-ItemProperty $advPath -Name HideFileExt -Value 0
}

# Show seconds in taskbar clock
if ($adv.ShowSecondsInSystemClock -ne 1) {
    Write-Host "Showing seconds in taskbar clock..." -ForegroundColor Yellow
    Set-ItemProperty $advPath -Name ShowSecondsInSystemClock -Value 1
}

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

# Dark theme
& $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\bin\toggle-theme.ps1" dark

# Phase 3: mise install (dev tool runtimes)
if (Get-Command mise -ea 0) {
    Write-Host "`nInstalling mise tools..." -ForegroundColor Cyan
    mise install --yes
}

# Phase 4: large apps (non-fatal)
& $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\install\scoop.ps1" -OnlyLarge

# Phase 5: startup (skip if apps already running)
if (!(Get-Process wezterm-gui -ea 0)) {
    Write-Host "`nLaunching startup apps..." -ForegroundColor Cyan
    & $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\startup\manager.ps1"
} else {
    Write-Host "`nStartup apps already running, skipped." -ForegroundColor Gray
}

if ($script:needsReboot) {
    Write-Host "`nDone! Reboot required (UTF-8 locale changed). Restart terminal after reboot." -ForegroundColor Yellow
} else {
    Write-Host "`nDone! Restart terminal to apply." -ForegroundColor Green
}
