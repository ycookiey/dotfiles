$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
$WingetFile = "$ScriptDir\wingetfile.json"

if (!(gcm winget -ea 0)) {
    wh "winget not found. Skipping winget setup." -Fg Yellow
    return
}

$json = gc $WingetFile -Raw | ConvertFrom-Json

foreach ($app in $json.apps) {
    $check = winget list --id $app.Id -e 2>$null
    if ($LASTEXITCODE -eq 0 -and $check -match [regex]::Escape($app.Id)) {
        continue
    }
    wh "Installing $($app.Id)..." -Fg Cyan
    winget install --id $app.Id -e --accept-source-agreements --accept-package-agreements
}

wh "`nWinget setup complete." -Fg Green
