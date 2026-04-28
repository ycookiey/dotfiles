# Windows Startup Manager

. "$PSScriptRoot\..\pwsh\aliases.ps1"
. "$PSScriptRoot\popup.ps1"

$script:LogFile = $null

function Initialize-Logging {
    param([string]$Dir)
    mkd $Dir
    $script:LogFile = "$Dir\startup-$(Get-Date -Format 'yyyyMMdd-HHmmss').log"
    Write-Log "Startup Manager started"
}

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")
    $line = "[$(Get-Date -Format 'HH:mm:ss')] [$Level] $Message"
    if ($script:LogFile) { ac $script:LogFile $line -Enc UTF8 }
}

function Start-Unelevated([scriptblock]$Launch) {
    $aliasDir = "$PSScriptRoot\..\pwsh"
    $inner = ". '$aliasDir\aliases.ps1'; . '$aliasDir\generated-aliases.ps1'; " + $Launch.ToString().Trim()
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($inner))
    & runas /trustlevel:0x20000 "pwsh -NoProfile -W Hidden -NonInteractive -EncodedCommand $encoded"
}

function Start-App-Logged {
    param([string]$Name, [scriptblock]$Launch, [switch]$Elevated)
    Write-Log "Starting: $Name"
    Show-AppStatus -AppName $Name -Status "Starting"
    try {
        if (!$Elevated -and (isadmin)) {
            Start-Unelevated $Launch
        } else {
            & $Launch
        }
        Write-Log "  Started: $Name"
        Show-AppStatus -AppName $Name -Status "Success"
    } catch {
        Write-Log "  Failed: $($_.Exception.Message)" "ERROR"
        Show-AppStatus -AppName $Name -Status "Error"
    }
}

Initialize-Logging -Dir "$PSScriptRoot\..\logs"
Show-PopupNotification "Startup" "Starting apps" 2000

# --- High priority ---
Start-App-Logged "WezTerm" { wezterm-gui }
Start-App-Logged "AutoHotkey" -Elevated {
    start "$HOME\scoop\shims\autohotkey.exe" -Arg "C:\Main\Project\dotfiles\autohotkey\shortcuts.ahk"
}
sleep -Seconds 1
Start-App-Logged "yClocky" { yclocky }
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
