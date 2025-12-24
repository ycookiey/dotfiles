# register-startup.ps1
# Register startup.vbs to Windows Registry Run key
# TODO: Integrate into setup.ps1

$vbsPath = "$PSScriptRoot\startup.vbs"
$regPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$regName = "DotfilesStartup"

Write-Host "Startup Registration" -ForegroundColor Cyan
Write-Host ""

# Check if VBScript exists
if (!(Test-Path $vbsPath)) {
    Write-Host "ERROR: startup.vbs not found" -ForegroundColor Red
    exit 1
}

# Check if already registered
$current = Get-ItemProperty -Path $regPath -Name $regName -ErrorAction SilentlyContinue
if ($current) {
    Write-Host "Already registered: $($current.$regName)" -ForegroundColor Yellow
    $overwrite = Read-Host "Overwrite? (Y/N)"
    if ($overwrite -ne 'Y') { exit 0 }
}

# Register to registry
$command = "wscript.exe `"$vbsPath`""
Set-ItemProperty -Path $regPath -Name $regName -Value $command -Type String

Write-Host ""
Write-Host "SUCCESS: Registered!" -ForegroundColor Green
Write-Host "Command: $command" -ForegroundColor Gray
Write-Host ""
Read-Host "Press Enter to exit"
