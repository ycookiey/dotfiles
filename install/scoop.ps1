param(
    [switch]$SkipLarge,
    [switch]$OnlyLarge
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
$ScoopFile = "$ScriptDir\scoopfile.json"

# Scoop インストール
if (!(gcm scoop -ea 0)) {
    wh "Installing Scoop..." -Fg Cyan
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
        wh "Adding bucket: $($b.Name)" -Fg Cyan
        scoop bucket add $b.Name $b.Source
    }
}

# アプリインストール（大きいアプリを後回し）
$orderFile = "$ScriptDir\install-order.json"
if (Test-Path $orderFile) {
    $order = gc $orderFile -Raw | ConvertFrom-Json
    $large = $order.large
    $apps = $json.apps | Sort-Object { if ($_.Name -in $large) { 1 } else { 0 } }
    if ($SkipLarge) { $apps = $apps | ? { $_.Name -notin $large } }
    if ($OnlyLarge) { $apps = $apps | ? { $_.Name -in $large } }
    $installed = scoop list 2>$null | % { $_.Name }
    $failed = @()
    foreach ($app in $apps) {
        if ($app.Name -notin $installed) {
            wh "Installing $($app.Name)..." -Fg Cyan
            if ($OnlyLarge) {
                try { scoop install $app.Name }
                catch {
                    wh "  Skipped (failed): $($app.Name) — $_" -Fg Yellow
                    $failed += $app.Name
                }
            } else {
                scoop install $app.Name
            }
        }
    }
    if ($failed.Count -gt 0) {
        wh "`nFailed large apps: $($failed -join ', ')" -Fg Yellow
        wh "Retry later: scoop install $($failed -join ' ')" -Fg Yellow
    }
} else {
    scoop import $ScoopFile
}

wh "`nScoop setup complete." -Fg Green
wh "Note: node, python, java, awscli は mise で管理 (mise.toml)" -Fg Yellow
