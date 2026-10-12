# Windows Startup Manager
#
# Two scheduled tasks (startup/register.ps1) run this script:
# - CustomStartupManager (at logon, highest privileges, -Part Elevated):
#   starts the apps that need administrator rights, then runs the next task.
# - CustomStartupManager-User (on demand, limited, -Part User): starts every
#   other app with the normal user token, the same one an app started from
#   the Start menu gets, so UAC elevation from those apps works.
# (Lowering rights with `runas /trustlevel` instead keeps High integrity but
# strips the Administrators group and privileges for good: elevating from
# such an app never gets full rights.)
param([ValidateSet('Elevated', 'User')][string]$Part = 'Elevated')

. "$PSScriptRoot\..\pwsh\aliases.ps1"
. "$PSScriptRoot\..\generated-aliases.ps1"
. "$PSScriptRoot\popup.ps1"

$UserTaskName = "CustomStartupManager-User"

$script:LogFile = $null

function Initialize-Logging {
    param([string]$Dir)
    mkd $Dir
    $script:LogFile = "$Dir\startup-$(Get-Date -Format 'yyyyMMdd-HHmmss')-$($Part.ToLower()).log"
    Write-Log "Startup Manager started ($Part, admin: $(isadmin))"
}

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")
    $line = "[$(Get-Date -Format 'HH:mm:ss')] [$Level] $Message"
    if ($script:LogFile) { ac $script:LogFile $line -Enc UTF8 }
}

function Start-App-Logged {
    param([string]$Name, [scriptblock]$Launch)
    Write-Log "Starting: $Name"
    Show-AppStatus -AppName $Name -Status "Starting"
    try {
        & $Launch
        Write-Log "  Started: $Name"
        Show-AppStatus -AppName $Name -Status "Success"
    } catch {
        Write-Log "  Failed: $($_.Exception.Message)" "ERROR"
        Show-AppStatus -AppName $Name -Status "Error"
    }
}

Initialize-Logging -Dir "$PSScriptRoot\..\logs"

if ($Part -eq 'Elevated') {
    Show-PopupNotification "Startup" "Starting apps" 2000
    # Elevated so it can focus UAC dialogs (autohotkey/shortcuts.ahk).
    Start-App-Logged "AutoHotkey" {
        start "$HOME\scoop\shims\autohotkey.exe" -Arg "C:\Main\Project\dotfiles\autohotkey\shortcuts.ahk"
    }
    sleep -Seconds 1
    Write-Log "Starting the user task: $UserTaskName"
    try {
        Start-ScheduledTask -TaskName $UserTaskName -ea Stop
    } catch {
        # Never fall back to starting the other apps from here: they would
        # run with administrator rights.
        Write-Log "  Failed: $($_.Exception.Message)" "ERROR"
        Show-PopupNotification "Error" "$UserTaskName did not start (startup/register.ps1)" 5000
    }
    return
}

# --- High priority ---
Start-App-Logged "WezTerm" { wezterm-gui }
Start-App-Logged "yClocky" { yclocky }
Start-App-Logged "ybasey-mirror" {
    start "C:\Main\Project\ybasey-mirror\target\release\ybasey-mirror.exe" `
        -Arg "--config","C:\Main\Project\ybasey-mirror\config.toml" `
        -WindowStyle Hidden
}
Start-App-Logged "ytasky TUI" { wezterm start --always-new-process -- ytasky }
sleep -Seconds 2

# --- Medium priority ---
Start-App-Logged "HideTaskBar" { htb }
Start-App-Logged "yCursory" { ycursory }
Start-App-Logged "yStrokey" { ystrokey }
Start-App-Logged "yMonitory" { ymonitory }
Start-App-Logged "yhwndy" { yhwndy }
sleep -Seconds 2
Start-App-Logged "Notion Calendar" { cal }
sleep -Seconds 2

# --- Low priority ---
Start-App-Logged "LGHUB" { lghub }
Start-App-Logged "NVIDIA Broadcast" { nvbc }
Start-App-Logged "Discord" { dis }

Write-Log "Startup complete"
Show-PopupNotification "Complete" "Startup complete" 3000
