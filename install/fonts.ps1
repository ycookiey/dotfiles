$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"

$FontsDir = "$env:LOCALAPPDATA\Microsoft\Windows\Fonts"
mkd $FontsDir

# HackGen Console NF — GitHub latest release からインストール
$fontCheck = gci $FontsDir -Filter "HackGenConsoleNF-Regular.ttf" -ea 0
if ($fontCheck) {
    wh "HackGen Console NF: already installed" -Fg Green
    return
}

wh "Installing HackGen Console NF..." -Fg Cyan
$tmp = "$env:TEMP\hackgen_nf"
rm $tmp -Recurse -Force -ea 0
mkd $tmp

# gh CLI で latest リリースの NF zip をダウンロード
gh release download --repo yuru7/HackGen --pattern "HackGen_NF_*.zip" --dir $tmp
$zip = (gci "$tmp\HackGen_NF_*.zip")[0]
Expand-Archive $zip.FullName "$tmp\extracted"

# HackGenConsoleNF のみインストール（per-user）
$RegKey = "HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts"
if (!(tp $RegKey)) { [void](ni $RegKey -Force) }

$ttfs = gci "$tmp\extracted" -Recurse -Filter "HackGenConsoleNF-*.ttf"
foreach ($f in $ttfs) {
    $dest = "$FontsDir\$($f.Name)"
    cp $f.FullName $dest -Force
    # フォント名をレジストリに登録
    $fontName = [IO.Path]::GetFileNameWithoutExtension($f.Name) -replace '-', ' '
    sp $RegKey "$fontName (TrueType)" $dest
}

rm $tmp -Recurse -Force -ea 0
wh "HackGen Console NF: installed ($($ttfs.Count) files)" -Fg Green
