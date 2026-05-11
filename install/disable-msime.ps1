$ErrorActionPreference = 'Stop'

$GoogleClsid = '{D5A86FD5-5308-47EA-AD16-9C4EB160EC3C}'
$LangProfilePath = "HKLM:\SOFTWARE\Microsoft\CTF\TIP\$GoogleClsid\LanguageProfile\0x00000411"

if (!(Test-Path $LangProfilePath)) {
    Write-Host "Google Japanese IME 未インストール。スキップ" -Fo Yellow
    return
}

$profileGuid = (Get-ChildItem $LangProfilePath | Select-Object -First 1).PSChildName
$googleTip = "0411:$GoogleClsid$profileGuid"

$list = Get-WinUserLanguageList
$ja = $list | ? { $_.LanguageTag -in 'ja','ja-JP' }
if (!$ja) {
    $list.Add('ja-JP')
    $ja = $list | ? { $_.LanguageTag -in 'ja','ja-JP' }
}

if ($ja.InputMethodTips.Count -eq 1 -and $ja.InputMethodTips[0] -eq $googleTip) {
    Write-Host "Microsoft IME は既に無効化済み" -Fo Green
    return
}

$ja.InputMethodTips.Clear()
$ja.InputMethodTips.Add($googleTip)
Set-WinUserLanguageList $list -Force

Write-Host "Microsoft IME を入力候補から除外（Google IME のみ）。サインアウト後に完全反映" -Fo Green
