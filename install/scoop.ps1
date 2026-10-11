param(
    [switch]$SkipLarge,
    [switch]$OnlyLarge,
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

# Scoop 設定
scoop config update_nightly true

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
