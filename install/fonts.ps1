$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"

$FontsDir = "$env:LOCALAPPDATA\Microsoft\Windows\Fonts"
mkd $FontsDir

# HackGen Console NF — GitHub latest release からインストール
$fontCheck = gci $FontsDir -Filter "HackGenConsoleNF-Regular.ttf" -ea 0
if ($fontCheck) {
    wh "HackGen Console NF: already installed" -Fo Green
    return
}

wh "Installing HackGen Console NF..." -Fo Cyan
$tmp = "$env:TEMP\hackgen_nf"
rm $tmp -Recurse -Force -ea 0
mkd $tmp

# GitHub API で latest リリースの NF zip をダウンロード（gh 認証不要。新 PC では gh 未認証）
# 失敗時は警告のみ: フォントが無くても後続の setup は続けられる
$ProgressPreference = 'SilentlyContinue'
try {
    $rel = Invoke-RestMethod "https://api.github.com/repos/yuru7/HackGen/releases/latest"
    $asset = $rel.assets | ? name -like 'HackGen_NF_*.zip' | select -First 1
    if (!$asset) { throw "HackGen_NF_*.zip not found in release $($rel.tag_name)" }
    $zip = "$tmp\$($asset.name)"
    Invoke-WebRequest $asset.browser_download_url -OutFile $zip -UseBasicParsing
    Expand-Archive $zip "$tmp\extracted"
} catch {
    wh "HackGen Console NF: download failed, skipped ($_)" -Fo Yellow
    rm $tmp -Recurse -Force -ea 0
    return
}

# HackGenConsoleNF のみインストール（per-user）
$RegKey = "HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts"
if (!(tp $RegKey)) { [void](ni $RegKey -Force) }

$ttfs = gci "$tmp\extracted" -Recurse -Filter "HackGenConsoleNF-*.ttf"
if (!$ttfs) {
    wh "HackGen Console NF: no HackGenConsoleNF-*.ttf in $($asset.name), skipped" -Fo Yellow
    rm $tmp -Recurse -Force -ea 0
    return
}
foreach ($f in $ttfs) {
    $dest = "$FontsDir\$($f.Name)"
    cp $f.FullName $dest -Force
    # フォント名をレジストリに登録
    $fontName = [IO.Path]::GetFileNameWithoutExtension($f.Name) -replace '-', ' '
    sp $RegKey "$fontName (TrueType)" $dest
}

rm $tmp -Recurse -Force -ea 0
wh "HackGen Console NF: installed ($($ttfs.Count) files)" -Fo Green
