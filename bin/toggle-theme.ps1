#Requires -Version 7.0
# toggle-theme [dark|light] — Windows ダーク/ライトモード + デスクトップ背景色トグル
param(
    [Parameter(Position = 0)]
    [ValidateSet('dark', 'light')]
    [string]$Mode
)

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\..\aliases.ps1"

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

# 単色BMP生成 → 壁紙に設定
$bmpPath = "$env:LOCALAPPDATA\theme-bg.bmp"
$color = if ($isDark) { [System.Drawing.Color]::Black } else { [System.Drawing.Color]::White }
$bmp = [System.Drawing.Bitmap]::new(8, 8)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear($color)
$g.Dispose()
$bmp.Save($bmpPath, [System.Drawing.Imaging.ImageFormat]::Bmp)
$bmp.Dispose()

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Wallpaper {
    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    public static extern int SystemParametersInfo(int uAction, int uParam, string lpvParam, int fuWinIni);
}
'@
# SPI_SETDESKWALLPAPER=0x14, SPIF_UPDATEINIFILE=0x01, SPIF_SENDWININICHANGE=0x02
[void][Wallpaper]::SystemParametersInfo(0x14, 0, $bmpPath, 0x01 -bor 0x02)

# Claude Code テーマ切り替え
$claudeJson = "$HOME\.claude-3\.claude.json"
if (tp $claudeJson) {
    $json = gc $claudeJson -Raw | ConvertFrom-Json
    $json.theme = $isDark ? 'dark' : 'light'
    $json | ConvertTo-Json -Depth 10 > $claudeJson
}

wh "$($isDark ? 'Dark' : 'Light') mode" -ForegroundColor ($isDark ? 'DarkCyan' : 'Yellow')
