# uv tool で導入する CLI ツール（uv-tools.json ベース）
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"
$ToolsFile = "$ScriptDir\uv-tools.json"

if (!(gcm uv -ea 0)) {
    wh "uv not found. Skipping uv tools." -Fo Yellow
    exit 0
}

$json = gc $ToolsFile -Raw | ConvertFrom-Json

# 導入済みツールをキャッシュ（行頭のツール名で判定）
$installed = (uv tool list 2>$null) -join "`n"

# 失敗 (非 0) は集めて最後に一覧を出す。S キーのスキップ ($null) は失敗に数えない
$failed = @()
foreach ($tool in $json.tools) {
    if ($installed -match ('(?m)^' + [regex]::Escape($tool) + '\s')) {
        continue
    }
    $rc = Invoke-Skippable "Installing $tool (uv tool)" uv "tool install $tool"
    if ($null -ne $rc -and $rc -ne 0) {
        wh "  Failed: $tool (exit $rc)" -Fo Yellow
        $failed += $tool
    }
}

if ($failed.Count -gt 0) {
    wh "`nFailed uv tools: $($failed -join ', ')" -Fo Yellow
    wh "Retry:" -Fo Yellow
    foreach ($t in $failed) { wh "  uv tool install $t" -Fo Yellow }
    exit 1
}

wh "`nuv tools setup complete." -Fo Green
# 途中の native コマンド (uv tool list) の $LASTEXITCODE を呼び出し元に残さない
exit 0
