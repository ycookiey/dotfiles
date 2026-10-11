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

# Claude Code テーマ切り替え
$claudeJson = "$HOME\.claude-3\.claude.json"
if (tp $claudeJson) {
    $json = gc $claudeJson -Raw | ConvertFrom-Json
    $val = $isDark ? 'dark' : 'light'
    if ($json.PSObject.Properties['theme']) { $json.theme = $val }
    else { $json | Add-Member theme $val }
    $json | ConvertTo-Json -Depth 10 > $claudeJson
}

wh "$($isDark ? 'Dark' : 'Light') mode" -ForegroundColor ($isDark ? 'DarkCyan' : 'Yellow')
