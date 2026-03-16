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

# --- Scoopfile ---
$scoopfile = "$Dot\install\scoopfile.json"
if (Get-Command scoop -ea 0) {
    $export = scoop export | ConvertFrom-Json
    foreach ($b in $export.buckets) {
        $b.PSObject.Properties.Remove('Updated')
        $b.PSObject.Properties.Remove('Manifests')
    }
    foreach ($app in $export.apps) {
        $app.PSObject.Properties.Remove('Version')
        $app.PSObject.Properties.Remove('Updated')
        $app.PSObject.Properties.Remove('Info')
    }
    $new = $export | ConvertTo-Json -Depth 3
    $old = if ([IO.File]::Exists($scoopfile)) { [IO.File]::ReadAllText($scoopfile).TrimEnd() } else { '' }
    if ($new -ne $old) {
        $new | Set-Content $scoopfile
        $synced = $true
    }
}

# --- Wingetfile ---
$wingetfile = "$Dot\install\wingetfile.json"
if (Get-Command winget -ea 0) {
    $tmp = "$env:TEMP\winget-export-$PID.json"
    try {
        winget export -o $tmp --accept-source-agreements 2>$null | Out-Null
        if ([IO.File]::Exists($tmp) -and (Get-Item $tmp).Length -gt 0) {
            $export = Get-Content $tmp -Raw | ConvertFrom-Json
            # winget ソースのパッケージのみ抽出（msstore・システムは除外）
            $apps = @()
            foreach ($src in $export.Sources) {
                if ($src.SourceDetails.Name -eq 'winget') {
                    $apps += $src.Packages | ForEach-Object { @{ Id = $_.PackageIdentifier } }
                }
            }
            $apps = @($apps | Sort-Object { $_.Id })
            $new = @{ apps = $apps } | ConvertTo-Json -Depth 3
            $old = if ([IO.File]::Exists($wingetfile)) { [IO.File]::ReadAllText($wingetfile).TrimEnd() } else { '' }
            if ($new -ne $old) {
                $new | Set-Content $wingetfile
                $synced = $true
            }
        }
    } finally {
        Remove-Item $tmp -ea 0
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
