# startup/register.ps1 -Action Register|Unregister
param(
    [Parameter(Mandatory)]
    [ValidateSet("Register", "Unregister")]
    [string]$Action
)

. "$PSScriptRoot\..\pwsh\aliases.ps1"

# Two tasks (startup/manager.ps1): the logon task runs with highest
# privileges and starts only what needs them, then runs the user task, which
# starts everything else with the normal (limited) user token.
$TaskName   = "CustomStartupManager"
$UserTask   = "CustomStartupManager-User"
$Pwsh       = (Get-Command pwsh).Source
$ScriptPath = "$PSScriptRoot\manager.ps1"
$RegKey     = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"

# Every 5 minutes, stops processes left with no parent (bin/reap-orphans.ps1).
# conhost --headless keeps a console window from flashing up each time.
$ReaperTask   = "OrphanReaper"
$ReaperScript = (Resolve-Path "$PSScriptRoot\..\bin\reap-orphans.ps1").Path

function Register-Reaper {
    $action    = New-ScheduledTaskAction -Execute "conhost.exe" -Argument "--headless `"$Pwsh`" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$ReaperScript`""
    $trigger   = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval ([timespan]'0:5:0')
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -RunLevel Limited -LogonType Interactive
    $settings  = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([timespan]'0:2:0') -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    [void](Register-ScheduledTask -TaskName $ReaperTask -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force)
    wh "Registered: $ReaperTask (TaskScheduler)" -Fo Green
}

function Register-Startup {
    if (!(tp $ScriptPath)) {
        wh "Error: manager.ps1 not found" -Fo Red
        exit 1
    }

    if (!(isadmin)) { elevate "-ExecutionPolicy Bypass -File `"$PSCommandPath`" -Action Register" }

    # 旧レジストリ登録を解除
    if (gp $RegKey -Name $TaskName -ea 0) {
        rp $RegKey -Name $TaskName
        wh "Removed old registry entry" -Fo Gray
    }

    # Shared by both tasks. Priority 4 = NORMAL_PRIORITY_CLASS. The default 7 (BelowNormal) is
    # inherited by every app manager.ps1 starts (AHK hotkeys, and everything launched from them).
    # The battery switches keep the tasks from being skipped on a laptop running on battery.
    $settings  = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([timespan]'0:5:0') -Priority 4 -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries

    # Registered first: the logon task runs it.
    $action    = New-ScheduledTaskAction -Execute $Pwsh -Argument "-ExecutionPolicy Bypass -W Hidden -NonInteractive -File `"$ScriptPath`" -Part User"
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -RunLevel Limited -LogonType Interactive
    [void](Register-ScheduledTask -TaskName $UserTask -Action $action -Principal $principal -Settings $settings -Force)
    wh "Registered: $UserTask (TaskScheduler, on demand)" -Fo Green

    $action    = New-ScheduledTaskAction -Execute $Pwsh -Argument "-ExecutionPolicy Bypass -W Hidden -NonInteractive -File `"$ScriptPath`" -Part Elevated"
    $trigger   = New-ScheduledTaskTrigger -AtLogOn
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -RunLevel Highest -LogonType Interactive
    [void](Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force)
    wh "Registered: $TaskName (TaskScheduler)" -Fo Green
    Register-Reaper
}

function Unregister-Startup {
    if (Get-ScheduledTask -TaskName $TaskName -ea 0) {
        Unregister-ScheduledTask -TaskName $TaskName -Con:$false
        wh "Unregistered: $TaskName" -Fo Green
    } else {
        wh "Not registered" -Fo Yellow
    }
    if (Get-ScheduledTask -TaskName $UserTask -ea 0) {
        Unregister-ScheduledTask -TaskName $UserTask -Con:$false
        wh "Unregistered: $UserTask" -Fo Green
    }
    if (Get-ScheduledTask -TaskName $ReaperTask -ea 0) {
        Unregister-ScheduledTask -TaskName $ReaperTask -Con:$false
        wh "Unregistered: $ReaperTask" -Fo Green
    }
}

switch ($Action) {
    "Register"   { Register-Startup }
    "Unregister" { Unregister-Startup }
}
