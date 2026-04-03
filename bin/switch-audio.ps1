#Requires -Version 7.0
#Requires -Modules AudioDeviceCmdlets
# switch-audio — デフォルト音声デバイス（入力/出力）をインタラクティブに選択・変更

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\..\pwsh\aliases.ps1"

$all = Get-AudioDevice -List
$playback = @($all | ? Type -eq 'Playback')
$recording = @($all | ? Type -eq 'Recording')

# 重複ベース名があればフル名、なければベース名を ShortName として付与
function Resolve-DisplayNames([object[]]$Devices) {
    $bases = $Devices | % {
        if ($_.Name -match '^(.+?)\s*\(') { $Matches[1].Trim() } else { $_.Name }
    }
    for ($i = 0; $i -lt $Devices.Count; $i++) {
        $dups = ($bases | ? { $_ -eq $bases[$i] }).Count
        $name = if ($dups -gt 1) { $Devices[$i].Name } else { $bases[$i] }
        $Devices[$i] | Add-Member ShortName $name -Force
    }
}

function Select-Device([string]$Label, [object[]]$Devices) {
    Resolve-DisplayNames $Devices
    if ($Devices.Count -eq 0) { return $null }

    $idx = 0
    for ($i = 0; $i -lt $Devices.Count; $i++) {
        if ($Devices[$i].Default) { $idx = $i; break }
    }

    $raw = $Host.UI.RawUI

    wh "`n$Label"
    # メニュー行分のバッファを確保してからスタート位置を逆算（スクロール対策）
    for ($i = 0; $i -lt $Devices.Count; $i++) { wh "" }
    $menuStartLine = $raw.CursorPosition.Y - $Devices.Count

    $drawDeviceLines = {
        param([int]$selectedIdx)
        $w = $raw.WindowSize.Width
        if ($w -lt 2) { $w = 80 }
        $maxCh = $w - 1
        for ($i = 0; $i -lt $Devices.Count; $i++) {
            $d = $Devices[$i]
            $mark = if ($d.Default) { '*' } else { ' ' }
            $line = "$mark $($i + 1)) $($d.ShortName)"
            if ($line.Length -gt $maxCh) {
                $take = [Math]::Max(0, $maxCh - 3)
                $line = $line.Substring(0, $take) + '...'
            }
            $padding = $line.PadRight($maxCh)
            [Console]::SetCursorPosition(0, $menuStartLine + $i)
            $body = if ($i -eq $selectedIdx) { "`e[7m$padding`e[0m" } else { $padding }
            wh $body -NoNewline
        }
    }

    & $drawDeviceLines $idx

    function Move-BelowMenu {
        [Console]::SetCursorPosition(0, $menuStartLine + $Devices.Count)
        wh ""
    }

    Move-BelowMenu

    while ($true) {
        $k = [Console]::ReadKey($true)
        $handled = $false
        $noMod = $k.Modifiers -eq [ConsoleModifiers]0
        if ($k.Key -eq [ConsoleKey]::DownArrow -or ($noMod -and $k.KeyChar -eq 'j')) {
            if ($idx -lt $Devices.Count - 1) { $idx++ }
            $handled = $true
        }
        elseif ($k.Key -eq [ConsoleKey]::UpArrow -or ($noMod -and $k.KeyChar -eq 'k')) {
            if ($idx -gt 0) { $idx-- }
            $handled = $true
        }
        elseif ($k.Key -eq [ConsoleKey]::Enter) {
            Move-BelowMenu
            return $Devices[$idx]
        }
        elseif ($k.Key -eq [ConsoleKey]::Escape) {
            Move-BelowMenu
            return $null
        }
        elseif ($noMod -and ($k.KeyChar -eq 'q' -or $k.KeyChar -eq 'Q')) {
            Move-BelowMenu
            return $null
        }
        elseif ($k.Key -ge [ConsoleKey]::D1 -and $k.Key -le [ConsoleKey]::D9) {
            $num = [int]$k.Key - [int][ConsoleKey]::D1 + 1
            if ($num -le $Devices.Count) {
                Move-BelowMenu
                return $Devices[$num - 1]
            }
        }
        elseif ($k.Key -ge [ConsoleKey]::NumPad1 -and $k.Key -le [ConsoleKey]::NumPad9) {
            $num = [int]$k.Key - [int][ConsoleKey]::NumPad1 + 1
            if ($num -le $Devices.Count) {
                Move-BelowMenu
                return $Devices[$num - 1]
            }
        }
        if ($handled) { & $drawDeviceLines $idx }
    }
}

$outPrev = $playback | ? Default | select -f 1
$inPrev = $recording | ? Default | select -f 1

$outSel = Select-Device '🔊 Output Device:' $playback
if ($outSel -and !$outSel.Default) {
    Set-AudioDevice -Index $outSel.Index | Out-Null
}

$inSel = Select-Device '🎤 Input Device:' $recording
if ($inSel -and !$inSel.Default) {
    Set-AudioDevice -Index $inSel.Index | Out-Null
}

# 結果表示
wh ""
if ($outSel -and !$outSel.Default) {
    wh "✔ Output: $($outPrev.ShortName) → $($outSel.ShortName)" -ForegroundColor Green
} else {
    wh "✔ Output: unchanged" -ForegroundColor DarkGray
}
if ($inSel -and !$inSel.Default) {
    wh "✔ Input: $($inPrev.ShortName) → $($inSel.ShortName)" -ForegroundColor Green
} else {
    wh "✔ Input: unchanged" -ForegroundColor DarkGray
}
