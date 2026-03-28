# Bitwarden CLI でシークレットを取得し、ローカルファイルに書き出す
# setup.ps1 から別ウィンドウで起動される（対話入力が必要なため）
$ErrorActionPreference = "Stop"
. "$(Split-Path $MyInvocation.MyCommand.Definition)\..\pwsh\aliases.ps1"
$LogFile = "$HOME\.claude\setup.log"

$secrets = @(
    @{ Name = 'GLM API Key'; Dest = "$HOME\.claude\.glm-api-key" }
    # TODO: 必要に応じてエントリを追加
)

# 取得が必要なシークレットがあるか確認
$needed = $secrets | ? { !(tp $_.Dest) }
if (!$needed) {
    wh "全てのシークレットは取得済み" -ForegroundColor Green
    sleep 1
    exit
}

# bw ログイン/アンロック
$status = (bw status 2>$null | ConvertFrom-Json).status
$session = $null
if ($status -eq 'unauthenticated') {
    wh "Bitwarden: ログインが必要" -ForegroundColor Cyan
    $session = bw login --raw
} elseif ($status -eq 'locked') {
    wh "Bitwarden: アンロックが必要" -ForegroundColor Cyan
    $session = bw unlock --raw
}

$bwArgs = @()
if ($session) { $bwArgs = @('--session', $session) }

foreach ($s in $needed) {
    $key = & bw get notes $s.Name @bwArgs 2>$null
    if ($key) {
        $dir = Split-Path $s.Dest
        if (!(tp $dir)) { mkd $dir }
        [IO.File]::WriteAllText($s.Dest, $key.Trim())
        wh "取得: $($s.Name)" -ForegroundColor Green
        "$(Get-Date) - Bitwarden: $($s.Name) fetched" >> $LogFile
    } else {
        wh "未検出: $($s.Name)" -ForegroundColor Yellow
        "$(Get-Date) - Warning: Bitwarden item '$($s.Name)' not found" >> $LogFile
    }
}

wh "`n完了。このウィンドウは自動で閉じます..." -ForegroundColor Green
sleep 2
