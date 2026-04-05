# startup/register.ps1 -Action Register|Unregister
param(
    [Parameter(Mandatory)]
    [ValidateSet("Register", "Unregister")]
    [string]$Action
)

. "$PSScriptRoot\..\pwsh\aliases.ps1"

$TaskName   = "CustomStartupManager"
$Pwsh       = (Get-Command pwsh).Source
$ScriptPath = "$PSScriptRoot\manager.ps1"
$RegKey     = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"

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

    $action    = New-ScheduledTaskAction -Execute $Pwsh -Argument "-ExecutionPolicy Bypass -W Hidden -NonInteractive -File `"$ScriptPath`""
    $trigger   = New-ScheduledTaskTrigger -AtLogOn
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -RunLevel Highest -LogonType Interactive
    $settings  = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([timespan]'0:5:0')
    [void](Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force)
    wh "Registered: $TaskName (TaskScheduler)" -Fo Green
}

function Unregister-Startup {
    if (Get-ScheduledTask -TaskName $TaskName -ea 0) {
        Unregister-ScheduledTask -TaskName $TaskName -Con:$false
        wh "Unregistered: $TaskName" -Fo Green
    } else {
        wh "Not registered" -Fo Yellow
    }
}

switch ($Action) {
    "Register"   { Register-Startup }
    "Unregister" { Unregister-Startup }
}
