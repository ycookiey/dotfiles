param(
    [switch]$SkipLarge,
    [switch]$OnlyLarge,
    # scoop のダウンロードに aria2 (多重接続) を使う。opt-in: 一部のミラー・プロキシで失敗する報告があり、
    # aria2 を入れると scoop 全体の設定として以後の install/update にも効くため
    [switch]$UseAria2,
    # 読む manifest。bootstrap は Phase 2 の dotcli sync (scoop export で上書き) 前のスナップショットを渡す
    [string]$ScoopFile = "$PSScriptRoot\scoopfile.json"
)

if ($SkipLarge -and $OnlyLarge) {
    throw "-SkipLarge and -OnlyLarge cannot be used together. Please specify only one of these switches."
}

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"

# Scoop インストール
if (!(gcm scoop -ea 0)) {
    wh "Installing Scoop..." -Fo Cyan
    Set-ExecutionPolicy RemoteSigned -Scope CurrentUser -Force
    irm get.scoop.sh | iex
}

# git（scoop bucket add に必要）
if (!(gcm git -ea 0)) {
    scoop install git
}

# バケット追加
$json = gc $ScoopFile -Raw | ConvertFrom-Json
$existing = scoop bucket list 2>$null | % { $_.Name }
foreach ($b in $json.buckets) {
    if ($b.Name -notin $existing) {
        wh "Adding bucket: $($b.Name)" -Fo Cyan
        scoop bucket add $b.Name $b.Source
    }
}

# scoop の config.json を scoop を起動せずに読む。無い・読めないときは $null
# 場所は scoop の lib/core.ps1 と同じ: <scoop root>\config.json (portable) があればそれ、
# 無ければ $env:XDG_CONFIG_HOME (未設定なら ~\.config) の scoop\config.json
$scoopRoot = if ($env:SCOOP) { $env:SCOOP } else { "$env:USERPROFILE\scoop" }
function Get-ScoopConfig([string]$Name) {
    $configHome = if ($env:XDG_CONFIG_HOME) { $env:XDG_CONFIG_HOME } else { "$env:USERPROFILE\.config" }
    $file = "$scoopRoot\config.json", "$configHome\scoop\config.json" | ? { tp $_ } | select -f 1
    if (!$file) { return $null }
    try { (gc $file -Raw | ConvertFrom-Json).$Name } catch { $null }
}

# Scoop 設定 (既に true なら scoop を起動しない)
if ((Get-ScoopConfig 'update_nightly') -ne $true) { scoop config update_nightly true }

# aria2: scoop は aria2 が入っていれば多重接続でダウンロードする (scoop 本体と同じく apps\aria2 で判定)
# 入らなかったときは従来のダウンロードで続行する
$aria2Exe = "$scoopRoot\apps\aria2\current\aria2c.exe"
if ($UseAria2) {
    if (!(tp $aria2Exe)) {
        wh "Installing aria2 (parallel downloads for scoop)..." -Fo Cyan
        try { scoop install aria2 } catch { wh "  $_" -Fo Yellow }
        if (!(tp $aria2Exe)) { wh "  aria2 install failed: continuing with the default downloader" -Fo Yellow }
    }
    if (tp $aria2Exe) {
        if ((Get-ScoopConfig 'aria2-warning-enabled') -ne $false) { scoop config aria2-warning-enabled false }
        if ((Get-ScoopConfig 'aria2-enabled') -eq $false) {
            wh "aria2 is disabled in scoop config. To use it: scoop config aria2-enabled true" -Fo Yellow
        } else {
            wh "scoop downloads use aria2. If downloads fail: scoop config aria2-enabled false" -Fo Yellow
        }
    }
}

# アプリインストール（大きいアプリを後回し）
# 失敗 (非 0) は集めて最後に一覧を出す。S キーのスキップ ($null) は失敗に数えない
$failed = @()
$orderFile = "$ScriptDir\install-order.json"
if (Test-Path $orderFile) {
    $order = gc $orderFile -Raw | ConvertFrom-Json
    $large = $order.large
    $apps = $json.apps | Sort-Object { if ($_.Name -in $large) { 1 } else { 0 } }
    if ($SkipLarge) { $apps = $apps | ? { $_.Name -notin $large } }
    if ($OnlyLarge) { $apps = $apps | ? { $_.Name -in $large } }
    $installed = scoop list 2>$null | % { $_.Name }
    foreach ($app in $apps) {
        if ($app.Name -notin $installed) {
            $scoopArgs = "-NoProfile -Command `"scoop install $($app.Name); if (`$LASTEXITCODE) { exit `$LASTEXITCODE }`""
            $rc = Invoke-Skippable "Installing $($app.Name)" pwsh $scoopArgs
            if ($null -ne $rc -and $rc -ne 0) {
                wh "  Failed: $($app.Name) (exit $rc)" -Fo Yellow
                $failed += $app.Name
            }
        }
    }
} else {
    scoop import $ScoopFile
    if ($LASTEXITCODE) { $failed += "scoop import $ScoopFile (exit $LASTEXITCODE)" }
}

if ($failed.Count -gt 0) {
    wh "`nFailed scoop apps: $($failed -join ', ')" -Fo Yellow
    if (Test-Path $orderFile) { wh "Retry: scoop install $($failed -join ' ')" -Fo Yellow }
    if ($UseAria2 -and (tp $aria2Exe)) { wh "If aria2 downloads failed: scoop config aria2-enabled false, then retry" -Fo Yellow }
    # 大きいアプリ (OnlyLarge) の失敗は非致命。それ以外は呼び出し元 (bootstrap Phase 1) が止まれるよう exit 1
    if (!$OnlyLarge) {
        wh "Scoop setup finished with failures." -Fo Red
        exit 1
    }
}

wh "`nScoop setup complete." -Fo Green
wh "Note: node, python, java, awscli は mise で管理 (~/.config/mise/config.toml)" -Fo Yellow
# 途中の native コマンド (scoop list 等) の $LASTEXITCODE を呼び出し元に残さない
exit 0
