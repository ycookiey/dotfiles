$ErrorActionPreference = 'Stop'
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition

$list = gc "$ScriptDir\webinstall.json" -Raw | ConvertFrom-Json
foreach ($app in $list) {
    if (gcm $app.cmd -ea 0) {
        wh "$($app.name) is already installed." -Fg Green
        continue
    }
    wh "Installing $($app.name)..." -Fg Cyan
    irm $app.url | iex
}
