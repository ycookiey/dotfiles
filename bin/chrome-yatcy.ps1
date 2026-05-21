#Requires -Version 5.1
# chrome-yatcy [-Port N] [-ProfileName NAME] — Chrome を CDP 有効 + 専用プロファイルで起動
#
# 例:
#   chrome-yatcy                                  # port=9222, profile=yatcy
#   chrome-yatcy -Port 9333 -ProfileName dev      # 別ポート/プロファイル
#   chrome-yatcy https://atcoder.jp/              # 追加引数は Chrome に渡す
#
# プロファイルは $env:LOCALAPPDATA\chrome-profiles\<ProfileName> に作成。
# 初回起動時はログイン状態を持たない素のプロファイル。
#
# 注意: パラメータ名に Profile/$Profile を使うと PowerShell 自動変数 $PROFILE と
# 衝突するため ProfileName を使用。-Port と短縮衝突する Alias も付けない。
param(
    [int]$Port = 9222,
    [string]$ProfileName = 'yatcy',
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Extra
)

$ErrorActionPreference = 'Stop'

# Chrome 探索 (Scoop shim 優先、Win 正規 install フォールバック)
$chrome = (Get-Command chrome -ErrorAction SilentlyContinue).Source
if (!$chrome) {
    $candidates = @(
        "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
        "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe"
    )
    $chrome = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (!$chrome) { throw "chrome.exe not found. Try: scoop install extras/googlechrome" }

# プロファイルディレクトリ ($env:LOCALAPPDATA\chrome-profiles\<ProfileName>)
$ProfileDir = Join-Path $env:LOCALAPPDATA "chrome-profiles\$ProfileName"
if (!(Test-Path $ProfileDir)) {
    [void](New-Item -ItemType Directory $ProfileDir -Force)
}

# Chrome 起動引数
# --remote-allow-origins=* は Chrome 136+ で CDP WebSocket 接続元検証が
# 厳格化された対策。ローカル限定なので broad に許可する。
$ChromeArgs = @(
    "--remote-debugging-port=$Port"
    "--remote-allow-origins=*"
    "--user-data-dir=$ProfileDir"
    "--no-first-run"
    "--no-default-browser-check"
) + $Extra

Write-Host "chrome-yatcy: port=$Port, profile=$ProfileName" -ForegroundColor Cyan
Write-Host "  exe : $chrome" -ForegroundColor DarkGray
Write-Host "  data: $ProfileDir" -ForegroundColor DarkGray

Start-Process $chrome -ArgumentList $ChromeArgs
