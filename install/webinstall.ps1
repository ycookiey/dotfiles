$ErrorActionPreference = 'Stop'
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"

$list = gc "$ScriptDir\webinstall.json" -Raw | ConvertFrom-Json
foreach ($app in $list) {
    if (gcm -CommandType Application -Name $app.cmd -ea 0) {
        wh "$($app.name) is already installed." -Fg Green
        continue
    }
    wh "Installing $($app.name)..." -Fg Cyan
    irm $app.url | iex
}
