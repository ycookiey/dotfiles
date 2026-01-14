#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Import existing Windows startup items to startup-apps.json.

.PARAMETER Force
    Skip confirmation prompt.

.EXAMPLE
    .\import-startup-items.ps1 -Force
#>

param(
    [string]$ConfigPath = "$PSScriptRoot\startup-apps.json",
    [string]$BackupPath = "$PSScriptRoot\startup-backup-$(Get-Date -Format 'yyyyMMdd-HHmmss').json",
    [switch]$Force
)

function Get-StartupItems {
    $items = @()

    # Registry
    $regPaths = @(
        @{Path = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"; Scope = "User"},
        @{Path = "HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce"; Scope = "User"; RunOnce = $true},
        @{Path = "HKLM:\Software\Microsoft\Windows\CurrentVersion\Run"; Scope = "Machine"},
        @{Path = "HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce"; Scope = "Machine"; RunOnce = $true}
    )

    foreach ($reg in $regPaths) {
        if (-not (Test-Path $reg.Path)) { continue }
        $props = Get-ItemProperty -Path $reg.Path -ErrorAction SilentlyContinue
        if (-not $props) { continue }

        $props.PSObject.Properties | Where-Object { $_.Name -notmatch '^PS' } | ForEach-Object {
            $path, $args = $_.Value, ""
            if ($_.Value -match '^"([^"]+)"(.*)$') { $path = $matches[1]; $args = $matches[2].Trim() }
            elseif ($_.Value -match '^(\S+)(.*)$') { $path = $matches[1]; $args = $matches[2].Trim() }

            $items += @{
                Name = $_.Name; Path = $path; Arguments = $args
                Source = "Registry"; SourcePath = $reg.Path; SourceScope = $reg.Scope
                IsRunOnce = $reg.RunOnce -eq $true; OriginalValue = $_.Value
            }
            Write-Host "  Found: $($_.Name)" -ForegroundColor Green
        }
    }

    # Startup Folders
    $folders = @(
        @{Path = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup"; Scope = "User"},
        @{Path = "$env:ProgramData\Microsoft\Windows\Start Menu\Programs\Startup"; Scope = "AllUsers"}
    )

    foreach ($folder in $folders) {
        if (-not (Test-Path $folder.Path)) { continue }
        Get-ChildItem -Path $folder.Path -Filter "*.lnk" -ErrorAction SilentlyContinue | ForEach-Object {
            $shell = New-Object -ComObject WScript.Shell
            $link = $shell.CreateShortcut($_.FullName)
            $items += @{
                Name = $_.BaseName; Path = $link.TargetPath; Arguments = $link.Arguments
                Source = "StartupFolder"; SourcePath = $_.FullName; SourceScope = $folder.Scope
                WorkingDirectory = $link.WorkingDirectory
            }
            [System.Runtime.Interopservices.Marshal]::ReleaseComObject($shell) | Out-Null
            Write-Host "  Found: $($_.BaseName)" -ForegroundColor Green
        }
    }

    return $items
}

function Add-ItemsToConfig {
    param([array]$Items, [string]$ConfigPath)
    $config = Get-Content -Path $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $added = 0

    foreach ($item in $Items) {
        if ($config.apps | Where-Object { $_.path -eq $item.Path }) {
            Write-Host "  Skip: $($item.Name) (exists)" -ForegroundColor Gray
            continue
        }
        $config.apps = @($config.apps) + [PSCustomObject]@{
            name = $item.Name; path = $item.Path; arguments = $item.Arguments
            priority = "Medium"; delay = 0; enabled = $true
            comment = "Imported from $($item.Source)"
        }
        $added++
        Write-Host "  Added: $($item.Name)" -ForegroundColor Yellow
    }

    $config | ConvertTo-Json -Depth 10 | Set-Content -Path $ConfigPath -Encoding UTF8
    return $added
}

function Remove-OriginalItems {
    param([array]$Items)
    foreach ($item in $Items) {
        try {
            if ($item.Source -eq "Registry") {
                Remove-ItemProperty -Path $item.SourcePath -Name $item.Name -ErrorAction Stop
            } else {
                Remove-Item -Path $item.SourcePath -Force -ErrorAction Stop
            }
            Write-Host "  Removed: $($item.Name)" -ForegroundColor Green
        } catch {
            Write-Host "  Failed: $($item.Name) - $($_.Exception.Message)" -ForegroundColor Red
        }
    }
}

# Main
Write-Host "Scanning startup items..." -ForegroundColor Cyan
$items = Get-StartupItems

if ($items.Count -eq 0) {
    Write-Host "No startup items found." -ForegroundColor Yellow
    exit 0
}

Write-Host "`nFound: $($items.Count) items" -ForegroundColor White

if (-not $Force) {
    $confirm = Read-Host "Import and remove originals? (Y/N)"
    if ($confirm -notin @('Y', 'y')) { Write-Host "Cancelled." -ForegroundColor Yellow; exit 0 }
}

# Backup
@{ Timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"; Items = $items } |
    ConvertTo-Json -Depth 10 | Set-Content -Path $BackupPath -Encoding UTF8
Write-Host "Backup: $BackupPath" -ForegroundColor Green

# Import
$added = Add-ItemsToConfig -Items $items -ConfigPath $ConfigPath
Remove-OriginalItems -Items $items

Write-Host "`nDone! Added: $added items" -ForegroundColor Green
