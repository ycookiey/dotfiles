<#
.SYNOPSIS
    Register or unregister Startup Manager to Windows startup (Task Scheduler).

.PARAMETER Action
    Action: Register or Unregister

.EXAMPLE
    .\register-startup.ps1 -Action Register
#>

param(
    [Parameter(Mandatory)]
    [ValidateSet("Register", "Unregister")]
    [string]$Action
)

# Require admin
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "Restarting as administrator..." -ForegroundColor Yellow
    Start-Process powershell.exe -ArgumentList "-ExecutionPolicy Bypass -File `"$PSCommandPath`" -Action $Action" -Verb RunAs
    exit
}

$TaskName = "CustomStartupManager"
$ScriptPath = Join-Path $PSScriptRoot "startup-manager.ps1"

function Register-Startup {
    if (-not (Test-Path $ScriptPath)) {
        Write-Host "Error: startup-manager.ps1 not found" -ForegroundColor Red
        exit 1
    }

    # Remove existing task
    $existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($existing) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    }

    # Create task
    $trigger = New-ScheduledTaskTrigger -AtLogOn
    $action = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-ExecutionPolicy Bypass -WindowStyle Hidden -NoProfile -File `"$ScriptPath`""
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Hours 1) -Priority 0
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited

    Register-ScheduledTask -TaskName $TaskName -Trigger $trigger -Action $action -Settings $settings -Principal $principal -Description "Custom Startup Manager" | Out-Null

    Write-Host "✓ Registered: $TaskName" -ForegroundColor Green
}

function Unregister-Startup {
    $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($task) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
        Write-Host "✓ Unregistered: $TaskName" -ForegroundColor Green
    } else {
        Write-Host "Not registered" -ForegroundColor Yellow
    }
}

# Main
switch ($Action) {
    "Register"   { Register-Startup }
    "Unregister" { Unregister-Startup }
}

# Show status
$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($task) {
    Write-Host "Status: $($task.State)" -ForegroundColor Gray
}
