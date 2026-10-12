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
        # The policy key does not exist by default, and Set-ItemProperty does not create it
        if (!(Test-Path $personalizationPath)) { $null = New-Item $personalizationPath -Force }
        Set-ItemProperty $personalizationPath -Name NoLockScreen -Value 1 -Force
    }
} else {
    Write-Host 'Skipping HKLM settings (UTF-8 locale / UAC / lock screen): run as admin to apply' -ForegroundColor DarkGray
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

# Snapshot the scoop manifest: Phase 2 (setup.ps1 -> dotcli sync) rewrites
# install/scoopfile.json from `scoop export`, which drops the large apps that
# Phase 4 has not installed yet. Phase 1 and Phase 4 both read this copy.
$scoopSnapshot = Join-Path $env:TEMP 'dotfiles-scoopfile.snapshot.json'
Copy-Item "$Dir\install\scoopfile.json" $scoopSnapshot -Force

# Phases 1-5 keep going when one fails: each failure is recorded with the command that reruns it,
# and the list is shown at the end. The steps above (scoop, git, pwsh, clone, snapshot) are what
# every phase needs, so they still stop the script.
# (Windows PowerShell 5.1 runs this file on a new PC: no PS7-only syntax, ASCII only.)
$script:Failures = [Collections.Generic.List[object]]::new()

# Runs one phase. A throw, or a non-zero $LASTEXITCODE left by its last native command or
# `pwsh -File`, is recorded with $Retry (a command to paste to rerun the phase). $Details returns
# extra lines to list under the failure.
function Invoke-Phase {
    param([string]$Name, [string]$Retry, [scriptblock]$Body, [scriptblock]$Details)
    $global:LASTEXITCODE = 0
    $reason = $null
    try {
        & $Body
        if ($global:LASTEXITCODE) { $reason = "exit code $global:LASTEXITCODE" }
    } catch {
        $reason = "$_"
    }
    if ($reason) {
        Write-Host "`n$Name failed: $reason (continuing)" -ForegroundColor Red
        $lines = @()
        if ($Details) { try { $lines = @(& $Details) } catch { } }
        $script:Failures.Add([pscustomobject]@{ Name = $Name; Reason = $reason; Details = $lines; Retry = $Retry })
    }
}

# The ' - Error: ' lines setup.ps1 wrote to its log in this run (it truncates the log on start).
function Get-SetupErrors([string]$LogFile) {
    if (!(Test-Path $LogFile)) { return }
    Get-Content $LogFile -Encoding UTF8 | Where-Object { $_ -match ' - Error: ' } |
        ForEach-Object { ($_ -split ' - Error: ', 2)[1] }
}

# Prints the failed phases with their rerun commands, then throws so the run ends non-zero.
# throw instead of exit: under `irm | iex` exit would close the console before the list is read,
# while an uncaught throw still makes `pwsh -File bootstrap.ps1` exit 1.
function Complete-Bootstrap {
    if (!$script:Failures.Count) { return }
    Write-Host "`n=== $($script:Failures.Count) failed (the rest ran) ===" -ForegroundColor Red
    foreach ($f in $script:Failures) {
        Write-Host "- $($f.Name): $($f.Reason)" -ForegroundColor Red
        foreach ($d in $f.Details) { Write-Host "    $d" -ForegroundColor Yellow }
        Write-Host "    rerun: $($f.Retry)" -ForegroundColor Cyan
    }
    throw "bootstrap finished with $($script:Failures.Count) failure(s): $(($script:Failures | ForEach-Object { $_.Name }) -join ', ')"
}

# pwsh may come from MSI/winget instead of scoop, so there may be no scoop shim
$pwsh = (Get-Command pwsh -CommandType Application -ea 0 | Select-Object -First 1).Source
if (!$pwsh) { $pwsh = Join-Path $scoopShims 'pwsh.exe' }
$runPs1 = 'pwsh -NoProfile -ExecutionPolicy Bypass -File'

# Phase 1: core apps (skip large). scoop.ps1 lists the failed apps itself.
Invoke-Phase 'Phase 1 (core scoop apps)' "$runPs1 `"$Dir\install\scoop.ps1`" -SkipLarge -ScoopFile `"$scoopSnapshot`"" {
    & $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\install\scoop.ps1" -SkipLarge -ScoopFile $scoopSnapshot
}

# Phase 2: config, symlinks, dotcli, startup (only needs core apps)
# setup.ps1 runs every step and exits 1 when any failed; the failed steps are read from its log.
Invoke-Phase 'Phase 2 (setup)' "$runPs1 `"$Dir\setup.ps1`"" {
    & $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\setup.ps1"
} { Get-SetupErrors "$HOME\.claude\setup.log" }

Invoke-Phase 'Dark theme' "$runPs1 `"$Dir\bin\toggle-theme.ps1`" dark" {
    & $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\bin\toggle-theme.ps1" dark
}

# Phase 3: mise install (dev tool runtimes)
if (Get-Command mise -ea 0) {
    Write-Host "`nInstalling mise tools..." -ForegroundColor Cyan
    Invoke-Phase 'Phase 3 (mise tools)' 'mise install --yes' { mise install --yes }
} else {
    # TODO: mise is in no install manifest, so a new PC always lands here. Either add mise to scoop
    # (and let sync record it) or drop mise.toml and install the runtimes through the manifests.
    Write-Host "`nmise not found: skipping runtime install (mise.toml tools are not installed)" -ForegroundColor Yellow
}

# Phase 3.5: uv tools (CLI tools via uv). uv-tools.ps1 lists the failed tools and exits 1.
if (Get-Command uv -ea 0) {
    Write-Host "`nInstalling uv tools..." -ForegroundColor Cyan
    Invoke-Phase 'Phase 3.5 (uv tools)' "$runPs1 `"$Dir\install\uv-tools.ps1`"" {
        & $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\install\uv-tools.ps1"
    }
}

# Phase 4: large apps. scoop.ps1 -OnlyLarge lists failed apps but exits 0 (large apps are optional).
Invoke-Phase 'Phase 4 (large scoop apps)' "$runPs1 `"$Dir\install\scoop.ps1`" -OnlyLarge -ScoopFile `"$scoopSnapshot`"" {
    & $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\install\scoop.ps1" -OnlyLarge -ScoopFile $scoopSnapshot
}

# Phase 5: startup (skip if apps already running)
if (!(Get-Process wezterm-gui -ea 0)) {
    Write-Host "`nLaunching startup apps..." -ForegroundColor Cyan
    Invoke-Phase 'Phase 5 (startup apps)' "$runPs1 `"$Dir\startup\manager.ps1`"" {
        & $pwsh -NoProfile -ExecutionPolicy Bypass -File "$Dir\startup\manager.ps1"
    }
} else {
    Write-Host "`nStartup apps already running, skipped." -ForegroundColor Gray
}

$doneColor = $(if ($script:Failures.Count) { 'Yellow' } else { 'Green' })
$doneText = $(if ($script:Failures.Count) { 'Finished with failures (listed below).' } else { 'Done!' })
if ($script:needsReboot) {
    Write-Host "`n$doneText Reboot required (UTF-8 locale changed). Restart terminal after reboot." -ForegroundColor Yellow
} else {
    Write-Host "`n$doneText Restart terminal to apply." -ForegroundColor $doneColor
}
Complete-Bootstrap
