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
    wh "`n$Label"
    for ($i = 0; $i -lt $Devices.Count; $i++) {
        $d = $Devices[$i]
        $mark = if ($d.Default) { '*' } else { ' ' }
        wh "$mark $($i + 1)) $($d.ShortName)"
    }

    $sel = Read-Host "Select [1-$($Devices.Count)] (Enter to skip)"
    if (!$sel) { return $null }
    if ($sel -notmatch '^\d+$') { wh "  Invalid" -ForegroundColor Red; return $null }

    $idx = [int]$sel - 1
    if ($idx -lt 0 -or $idx -ge $Devices.Count) {
        wh "  Invalid" -ForegroundColor Red
        return $null
    }
    return $Devices[$idx]
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
