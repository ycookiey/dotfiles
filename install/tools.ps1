$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
$ToolsFile = "$ScriptDir\tools.json"

if (!(Test-Path $ToolsFile)) { return }

$tools = gc $ToolsFile -Raw | ConvertFrom-Json
$installed = 0

foreach ($tool in $tools) {
    if (gcm $tool.cmd -ea 0) {
        wh "$($tool.name) already installed." -Fg Gray
        continue
    }
    wh "Installing $($tool.name)..." -Fg Cyan
    iex $tool.install
    $installed++
}

if ($installed -gt 0) {
    wh "`nNon-Scoop tools setup complete ($installed installed)." -Fg Green
}
