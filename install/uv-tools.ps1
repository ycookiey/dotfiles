# uv tool で導入する CLI ツール（uv-tools.json ベース）
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"
$ToolsFile = "$ScriptDir\uv-tools.json"

if (!(gcm uv -ea 0)) {
    wh "uv not found. Skipping uv tools." -Fo Yellow
    return
}

$json = gc $ToolsFile -Raw | ConvertFrom-Json

# 導入済みツールをキャッシュ（行頭のツール名で判定）
$installed = (uv tool list 2>$null) -join "`n"

foreach ($tool in $json.tools) {
    if ($installed -match ('(?m)^' + [regex]::Escape($tool) + '\s')) {
        continue
    }
    Invoke-Skippable "Installing $tool (uv tool)" uv "tool install $tool"
}

wh "`nuv tools setup complete." -Fo Green
