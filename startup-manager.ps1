# Windows Startup Manager
param([string]$ConfigPath = "$PSScriptRoot\startup-apps.json")

# Load popup notifications
. "$PSScriptRoot\startup-popup.ps1"

$script:LogFile = $null
$script:Config = $null

#region Logging

function Initialize-Logging {
    param([string]$Dir)
    if (-not (Test-Path $Dir)) { New-Item -Path $Dir -ItemType Directory -Force | Out-Null }
    $script:LogFile = Join-Path $Dir "startup-$(Get-Date -Format 'yyyyMMdd-HHmmss').log"
    Write-Log "Startup Manager started"
}

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")
    $line = "[$(Get-Date -Format 'HH:mm:ss')] [$Level] $Message"
    if ($script:LogFile) { Add-Content -Path $script:LogFile -Value $line -Encoding UTF8 }
}

#endregion

#region Application Startup

function Start-Application {
    param([PSCustomObject]$App)
    $path = [Environment]::ExpandEnvironmentVariables($App.path)
    $args = if ($App.arguments) { [Environment]::ExpandEnvironmentVariables($App.arguments) } else { "" }

    Write-Log "Starting: $($App.name)"

    if ($App.delay -gt 0) { Start-Sleep -Seconds $App.delay }

    Show-AppStatus -AppName $App.name -Status "Starting"

    try {
        if (-not (Test-Path $path)) { throw "Not found: $path" }

        $params = @{ FilePath = $path; ErrorAction = 'Stop' }
        if ($args) { $params['ArgumentList'] = $args }

        $proc = Start-Process @params -PassThru
        Start-Sleep -Milliseconds 500

        if ($proc.HasExited) { throw "Exited immediately (code: $($proc.ExitCode))" }

        Write-Log "  Started: PID=$($proc.Id)"
        Show-AppStatus -AppName $App.name -Status "Success"
        return $true
    } catch {
        Write-Log "  Failed: $($_.Exception.Message)" "ERROR"
        Show-AppStatus -AppName $App.name -Status "Error"
        return $false
    }
}

function Start-ApplicationsByPriority {
    param([array]$Apps, [string]$Priority)
    $apps = $Apps | Where-Object { $_.priority -eq $Priority -and $_.enabled }
    if ($apps.Count -eq 0) { return }

    Write-Log "Starting $Priority priority apps ($($apps.Count))"
    foreach ($app in $apps) { Start-Application -App $app | Out-Null }
}

#endregion

#region Main

function Start-Main {
    if (-not (Test-Path $ConfigPath)) { throw "Config not found: $ConfigPath" }

    $script:Config = Get-Content -Path $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Initialize-Logging -Dir $script:Config.settings.logDirectory

    $enabled = $script:Config.apps | Where-Object { $_.enabled }
    Write-Log "Apps to start: $($enabled.Count)"

    if ($enabled.Count -eq 0) { Write-Log "No apps"; return }

    Show-PopupNotification "Startup" "Starting $($enabled.Count) apps" 2000

    @("High", "Medium", "Low") | ForEach-Object {
        Start-ApplicationsByPriority -Apps $script:Config.apps -Priority $_
    }

    Write-Log "Startup complete"
    Show-PopupNotification "Complete" "Startup complete" 3000
}

try { Start-Main }
catch {
    Write-Log "Fatal: $($_.Exception.Message)" "ERROR"
    Show-PopupNotification "Error" $_.Exception.Message 4000
    throw
}

#endregion
