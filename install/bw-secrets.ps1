# Bitwarden CLI でシークレットを取得し、ローカルファイルに書き出す
# setup.ps1 から別ウィンドウで起動される（対話入力が必要なため）
$ErrorActionPreference = "Stop"
. "$(Split-Path $MyInvocation.MyCommand.Definition)\..\pwsh\aliases.ps1"
$Host.UI.RawUI.WindowTitle = 'dotfiles bitwarden secrets'
$LogFile = "$HOME\.claude\setup.log"

$secrets = @(
    @{ Name = 'GLM API Key'; Dest = "$HOME\.claude\.glm-api-key" }
    @{ Name = 'Tavily API Key'; Dest = "$HOME\.claude\.tavily-api-key" }
    @{ Name = 'GitHub SSH Key'; Dest = "$HOME\.ssh\id_ed25519"; Mode = 'SSHKey' }
)

# 取得が必要なシークレットがあるか確認
$needed = $secrets | ? { !(tp $_.Dest) }
if (!$needed) { exit }

# bw ログイン/アンロック
$status = (bw status 2>$null | ConvertFrom-Json).status
$session = $null
function Show-InputBanner($label) {
    wh ""
    wh " [入力待ち] $label ↓" -ForegroundColor Black -BackgroundColor Yellow
    try { [console]::beep(880, 200) } catch {}
}
if ($status -eq 'unauthenticated') {
    Show-InputBanner 'Bitwarden ログイン (メール/パスワード)'
    $session = bw login --raw
} elseif ($status -eq 'locked') {
    Show-InputBanner 'Bitwarden アンロック (マスターパスワード)'
    $session = bw unlock --raw
}
if (!$session -or $LASTEXITCODE -ne 0) {
    wh "Bitwarden: 認証失敗" -ForegroundColor Red
    "$(Get-Date) - Error: Bitwarden authentication failed" >> $LogFile
    exit 1
}
wh "Bitwarden: 認証成功" -ForegroundColor Green

$bwArgs = @('--session', $session)

# vault同期: 別PCで追加したアイテムをローカルキャッシュに反映
wh "  vault 同期中..." -ForegroundColor DarkGray
bw sync @bwArgs 2>&1 | Out-Null

foreach ($s in $needed) {
    $key = & bw get notes $s.Name @bwArgs 2>&1
    if ($LASTEXITCODE -eq 0 -and $key) {
        $dir = Split-Path $s.Dest
        if (!(tp $dir)) { mkd $dir }
        if ($s.Mode -eq 'SSHKey') {
            # SSH秘密鍵: LFのみ + 末尾改行必須(OpenSSHはCRLF不可), ACLは現ユーザのみ読み取り, 公開鍵を導出
            # bw native callは複数行を配列化するためLFでjoinしてから正規化
            $rawKey = if ($key -is [array]) { $key -join "`n" } else { [string]$key }
            $lfKey = ($rawKey -replace "`r", '').Trim() + "`n"
            [IO.File]::WriteAllText($s.Dest, $lfKey)
            icacls $s.Dest /inheritance:r | Out-Null
            icacls $s.Dest /grant:r "$($env:USERNAME):(R)" | Out-Null
            $pub = "$($s.Dest).pub"
            ssh-keygen -y -f $s.Dest > $pub 2>$null
        } else {
            [IO.File]::WriteAllText($s.Dest, $key.Trim())
        }
        wh "取得: $($s.Name)" -ForegroundColor Green
        "$(Get-Date) - Bitwarden: $($s.Name) fetched" >> $LogFile
    } else {
        wh "未検出: $($s.Name) — $key" -ForegroundColor Yellow
        "$(Get-Date) - Warning: Bitwarden item '$($s.Name)': $key" >> $LogFile
    }
}
