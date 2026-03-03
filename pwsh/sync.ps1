# sync.ps1 — バックグラウンドで毎セッション実行される冪等同期スクリプト
# 管理者権限不要。profile.ps1 からジョブとして起動される。
param([string]$Dot)

# --- Claude settings.json マージ ---
function Merge-ClaudeSettings([string]$Template, [string]$Target) {
    $tpl = Get-Content $Template -Raw | ConvertFrom-Json
    $existing = @{}
    if (Test-Path $Target) {
        try { $existing = Get-Content $Target -Raw | ConvertFrom-Json -AsHashtable } catch {}
    }
    foreach ($k in $tpl.PSObject.Properties.Name) { $existing[$k] = $tpl.$k }
    $existing | ConvertTo-Json -Depth 10 | Set-Content $Target
}

$settingsTemplate = "$Dot\claude\settings.json"
if ([IO.File]::Exists($settingsTemplate)) {
    foreach ($dir in @("$HOME\.claude") + @(Get-ChildItem "$HOME\.claude-*" -Dir -Force -ea 0)) {
        $target = "$dir\settings.json"
        Merge-ClaudeSettings $settingsTemplate $target
    }
}

# --- MCP servers ---
$claude = Get-Command claude -ea 0
if (!$claude) { return }

$mcpFile = "$Dot\claude\mcp-servers.json"
if (![IO.File]::Exists($mcpFile)) { return }

$vars = @{
    '{HOME}'         = $HOME
    '{PROJECTS}'     = Split-Path $Dot
    '{LOCALAPPDATA}' = $env:LOCALAPPDATA
    '{APPDATA}'      = $env:APPDATA
}

$raw = [IO.File]::ReadAllText($mcpFile)
foreach ($kv in $vars.GetEnumerator()) { $raw = $raw.Replace($kv.Key, $kv.Value.Replace('\', '/')) }
$servers = $raw | ConvertFrom-Json

foreach ($name in $servers.PSObject.Properties.Name) {
    $def = $servers.$name
    $cmd = $def.command
    if (!(Get-Command $cmd -ea 0) -and ![IO.File]::Exists($cmd)) { continue }

    $json = $def | ConvertTo-Json -Depth 5 -Compress
    $null = & $claude mcp remove -s user $name 2>&1
    $null = & $claude mcp add-json -s user $name $json 2>&1
    $synced = $true
}
if ($synced) { $true }
