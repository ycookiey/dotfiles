#Requires -Version 7.0
# toggle-theme [dark|light] — Windows ダーク/ライトモード + デスクトップ背景色トグル
param(
    [Parameter(Position = 0)]
    [ValidateSet('dark', 'light')]
    [string]$Mode
)

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\..\pwsh\aliases.ps1"

$ThemePath = 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize'

# 現在のモード検出（0=dark, 1=light）
$current = (Get-ItemProperty $ThemePath).AppsUseLightTheme

if (!$Mode) {
    $Mode = if ($current -eq 0) { 'light' } else { 'dark' }
}

$isDark = $Mode -eq 'dark'
$themeValue = if ($isDark) { 0 } else { 1 }

# テーマ切り替え
Set-ItemProperty $ThemePath -Name AppsUseLightTheme -Value $themeValue -Type Dword -Force
Set-ItemProperty $ThemePath -Name SystemUsesLightTheme -Value $themeValue -Type Dword -Force

# [Wallpaper] 型を用意する。Add-Type -TypeDefinition は実行のたびに C# をコンパイルするので、
# 初回だけ DLL に出力して $CacheDir に置き、以降は Assembly.LoadFrom で読む
# (Add-Type -Path も起動が重いので使わない)。DLL 名に pwsh の版とソースのハッシュを含め、
# どちらかが変われば作り直す。キャッシュが使えなければ従来どおりメモリ上でコンパイルする。
# 戻り値: loaded (既にある) | cached | compiled | inmemory
function Import-WallpaperType([string]$CacheDir) {
    if ('Wallpaper' -as [type]) { return 'loaded' }
    $src = @'
using System;
using System.Runtime.InteropServices;
public class Wallpaper {
    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    public static extern int SystemParametersInfo(int uAction, int uParam, string lpvParam, int fuWinIni);
}
'@
    try {
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($src))).Substring(0, 12)
        $dll = Join-Path $CacheDir "Wallpaper-$($PSVersionTable.PSVersion)-$hash.dll"
        if (Test-Path $dll) {
            try {
                [void][Reflection.Assembly]::LoadFrom($dll)
                if ('Wallpaper' -as [type]) { return 'cached' }
                throw "Wallpaper type not found in $dll"
            } catch {
                # 壊れた DLL は消して作り直す (ロード済みで消せなければ下の Move が失敗し inmemory に落ちる)
                Remove-Item $dll -Force -ErrorAction Ignore
            }
        }
        $null = New-Item $CacheDir -ItemType Directory -Force
        # 一時名に出力してから置く: 並行実行で別プロセスがロード中の DLL を上書きしない
        $tmp = Join-Path $CacheDir "Wallpaper.$PID.tmp.dll"
        Add-Type -TypeDefinition $src -OutputAssembly $tmp -OutputType Library
        try { Move-Item $tmp $dll -ErrorAction Stop }
        catch {
            Remove-Item $tmp -Force -ErrorAction Ignore
            if (!(Test-Path $dll)) { throw }  # 先に置かれた DLL があればそれを使う
        }
        [void][Reflection.Assembly]::LoadFrom($dll)
        if (!('Wallpaper' -as [type])) { throw "Wallpaper type not found in $dll" }
        # 古い版・古いソースの DLL を掃除 (ロード中で消せないものは次回に回す)
        try {
            Get-ChildItem $CacheDir -Filter 'Wallpaper-*.dll' |
                Where-Object FullName -ne $dll |
                Remove-Item -Force -ErrorAction Ignore
        } catch {}
        return 'compiled'
    } catch {
        Add-Type -TypeDefinition $src
        return 'inmemory'
    }
}

# 単色BMP → 壁紙に設定。BMP は色ごとの固定名で、無いときだけ作る (System.Drawing も初回だけ使う)
$cacheDir = "$env:LOCALAPPDATA\dotfiles"
$bmpPath = Join-Path $cacheDir "theme-bg-$Mode.bmp"
if (!(Test-Path $bmpPath)) {
    $null = New-Item $cacheDir -ItemType Directory -Force
    $tmpBmp = "$bmpPath.$PID.tmp"
    $color = if ($isDark) { [System.Drawing.Color]::Black } else { [System.Drawing.Color]::White }
    $bmp = [System.Drawing.Bitmap]::new(8, 8)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear($color)
    $g.Dispose()
    $bmp.Save($tmpBmp, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $bmp.Dispose()
    try { Move-Item $tmpBmp $bmpPath -ErrorAction Stop }
    catch {
        Remove-Item $tmpBmp -Force -ErrorAction Ignore
        if (!(Test-Path $bmpPath)) { throw }  # 並行実行で先に置かれた BMP があればそれを使う
    }
}

$null = Import-WallpaperType $cacheDir
# SPI_SETDESKWALLPAPER=0x14, SPIF_UPDATEINIFILE=0x01, SPIF_SENDWININICHANGE=0x02
[void][Wallpaper]::SystemParametersInfo(0x14, 0, $bmpPath, 0x01 -bor 0x02)

# Claude Code の .claude.json の最上位 theme を $Value にする。
# 実行中の Claude Code も同じファイルを書くので、読んでから書くまでを短くし、theme 以外の
# バイトは変えない: theme キーがあれば値だけ、無ければ先頭の { の直後に 1 行足す。
# 書く前に結果を ConvertFrom-Json で確かめ、だめなら従来どおりオブジェクトを丸ごと書き戻す。
# 戻り値: unchanged | replaced | inserted | rewritten
function Set-ClaudeTheme([string]$Path, [string]$Value) {
    $utf8 = [Text.UTF8Encoding]::new($false)
    $text = [IO.File]::ReadAllText($Path, $utf8)
    $json = $text | ConvertFrom-Json
    if ($json.PSObject.Properties['theme'] -and $json.theme -eq $Value) { return 'unchanged' }

    $new = $null; $how = $null
    $pat = '("theme"\s*:\s*")[^"\\]*(")'
    $hits = [regex]::Matches($text, $pat)
    if ($json.PSObject.Properties['theme']) {
        # 入れ子のオブジェクトにも theme があると最上位と区別できないので、1 か所のときだけ置換する
        if ($hits.Count -eq 1) {
            $new = [regex]::Replace($text, $pat, "`${1}$Value`${2}")
            $how = 'replaced'
        }
    } elseif ($text -match '^\s*\{(\r?\n)(\s*)"') {
        # 最初のキーと同じ改行・字下げで先頭に足す (空のオブジェクトや 1 行の JSON は下の書き戻しに回す)
        $nl = $Matches[1]; $indent = $Matches[2]
        $new = ([regex]'\{').Replace($text, "{$nl$indent`"theme`": `"$Value`",", 1)
        $how = 'inserted'
    }
    if ($new) {
        try { $ok = ($new | ConvertFrom-Json).theme -eq $Value } catch { $ok = $false }
        if (!$ok) { $new = $null }
    }
    if (!$new) {
        if ($json.PSObject.Properties['theme']) { $json.theme = $Value }
        else { $json | Add-Member theme $Value }
        $new = $json | ConvertTo-Json -Depth 100
        $how = 'rewritten'
    }
    [IO.File]::WriteAllText($Path, $new, $utf8)
    $how
}

# Claude Code テーマ切り替え (全アカウント ~/.claude-<N>)
$claudeTheme = $isDark ? 'dark' : 'light'
Get-ChildItem "$HOME\.claude-*" -Directory -ErrorAction Ignore |
    Where-Object Name -match '^\.claude-\d+$' |
    ForEach-Object {
        $claudeJson = Join-Path $_.FullName '.claude.json'
        if (!(tp $claudeJson)) { return }
        try { $null = Set-ClaudeTheme $claudeJson $claudeTheme }
        catch { wh "  $($_.Exception.Message) ($claudeJson)" -ForegroundColor DarkGray }
    }

wh "$($isDark ? 'Dark' : 'Light') mode" -ForegroundColor ($isDark ? 'DarkCyan' : 'Yellow')
