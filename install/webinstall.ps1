$ErrorActionPreference = 'Stop'
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"

$list = gc "$ScriptDir\webinstall.json" -Raw | ConvertFrom-Json
foreach ($app in $list) {
    if ((gcm -CommandType Application -Name $app.cmd -ea 0) -or (tp "$HOME\.local\bin\$($app.cmd).exe")) {
        wh "$($app.name) is already installed." -Fo Green
        continue
    }
    wh "Installing $($app.name)..." -Fo Cyan
    irm $app.url | iex
}
