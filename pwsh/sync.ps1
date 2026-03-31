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
    $buckets = $export.buckets | % { [ordered]@{ Name = $_.Name; Source = $_.Source } }
    $apps = $export.apps | % { [ordered]@{ Name = $_.Name; Source = $_.Source } }
    $new = [ordered]@{ buckets = $buckets; apps = $apps } | ConvertTo-Json -Depth 3
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
        $exitCode = $LASTEXITCODE
        if ($exitCode -eq 0 -and [IO.File]::Exists($tmp) -and (Get-Item $tmp).Length -gt 0) {
            $export = Get-Content $tmp -Raw | ConvertFrom-Json
            # winget ソースのパッケージのみ抽出（msstore・システムは除外）
            $apps = $export.Sources |
                Where-Object { $_.SourceDetails.Name -eq 'winget' } |
                ForEach-Object {
                    $_.Packages | ForEach-Object { @{ Id = $_.PackageIdentifier } }
                } |
                Sort-Object { $_.Id }
            $new = @{ apps = $apps } | ConvertTo-Json -Depth 3
            $old = if ([IO.File]::Exists($wingetfile)) { [IO.File]::ReadAllText($wingetfile).TrimEnd() } else { '' }
            if ($new -ne $old) {
                $new | Set-Content $wingetfile
                $synced = $true
            }
        }
    } catch {
        # Best-effort: ignore winget export / parse failures so sync.ps1 remains non-fatal.
    } finally {
        Remove-Item $tmp -ea 0
    }
}

# --- MCP servers ---
if (!(Get-Command node -ea 0)) { return }

$mcpFile = "$Dot\claude\mcp-servers.json"
if (![IO.File]::Exists($mcpFile)) { return }

$vars = @{
    '{HOME}'         = $HOME
    '{PROJECTS}'     = Split-Path $Dot
    '{LOCALAPPDATA}' = $env:LOCALAPPDATA
    '{APPDATA}'      = $env:APPDATA
}
$tavilyFile = "$HOME\.claude\.tavily-api-key"
if ([IO.File]::Exists($tavilyFile)) {
    $vars['{TAVILY_API_KEY}'] = [IO.File]::ReadAllText($tavilyFile).Trim()
}

$raw = [IO.File]::ReadAllText($mcpFile)
foreach ($kv in $vars.GetEnumerator()) { $raw = $raw.Replace($kv.Key, $kv.Value.Replace('\', '/')) }
$servers = $raw | ConvertFrom-Json

$resolved = [ordered]@{}
foreach ($name in $servers.PSObject.Properties.Name) {
    $def = $servers.$name
    $json = $def | ConvertTo-Json -Depth 5 -Compress
    if ($json -match '\{[A-Z_]+\}') { continue }
    $cmd = $def.command
    if (!(Get-Command $cmd -ea 0) -and ![IO.File]::Exists($cmd)) { continue }
    $resolved[$name] = $def
}
if ($resolved.Count -eq 0) { return }

$dirs = [System.Collections.Generic.List[string]]::new()
if ([IO.File]::Exists("$HOME\.claude\.claude.json")) { $dirs.Add("$HOME\.claude") }
foreach ($d in Get-ChildItem "$HOME\.claude-*" -Dir -Force -ea 0) {
    if ([IO.File]::Exists("$d\.claude.json")) { $dirs.Add($d.FullName) }
}
if ($dirs.Count -eq 0) { return }

$tmpPayload = "$env:TEMP\mcp-sync-$PID.json"
$tmpScript  = "$env:TEMP\mcp-sync-$PID.js"
try {
    $payloadJson = @{ servers = $resolved; dirs = @($dirs) } | ConvertTo-Json -Depth 10
    [IO.File]::WriteAllText($tmpPayload, $payloadJson)
    $js = @'
const fs=require("fs"),path=require("path");
const p=JSON.parse(fs.readFileSync(process.argv[2],"utf8"));
for(const dir of p.dirs){
const file=path.join(dir,".claude.json");
try{
const data=JSON.parse(fs.readFileSync(file,"utf8"));
if(JSON.stringify(data.mcpServers||{})!==JSON.stringify(p.servers)){
data.mcpServers=p.servers;
fs.writeFileSync(file,JSON.stringify(data,null,2));
}
}catch{}
}
'@
    [IO.File]::WriteAllText($tmpScript, $js)
    $null = node $tmpScript $tmpPayload 2>&1
    if ($LASTEXITCODE -eq 0) { $synced = $true }
} finally {
    Remove-Item $tmpPayload, $tmpScript -ea 0
}
if ($synced) { $true }
