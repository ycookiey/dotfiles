$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
$WingetFile = "$ScriptDir\wingetfile.json"

if (!(gcm winget -ea 0)) {
    wh "winget not found. Skipping winget setup." -Fg Yellow
    return
}

$json = gc $WingetFile -Raw | ConvertFrom-Json

# Cache list of installed apps once to avoid repeated winget invocations in the loop
$installedAppsText = ""
$installedAppsOutput = winget list 2>$null
if ($LASTEXITCODE -eq 0) {
    $installedAppsText = $installedAppsOutput -join "`n"
}

foreach ($app in $json.apps) {
    if ($installedAppsText -and $installedAppsText -match ("\b" + [regex]::Escape($app.Id) + "\b")) {
        continue
    }
    wh "Installing $($app.Id)..." -Fg Cyan
    winget install --id $app.Id -e --accept-source-agreements --accept-package-agreements
}

wh "`nWinget setup complete." -Fg Green
