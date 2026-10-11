# bin/reap-orphans.ps1 [-DryRun]
# Stops processes left behind with no parent: Claude Code starts hooks,
# the statusline and Bash tool commands through Git Bash, and when bash is
# stopped its children can stay suspended forever or keep a core busy
# (2026-10-11: 594 suspended jq, 58 suspended claude-statusline, hook bash
# processes spinning for hours). Run every 5 minutes by Task Scheduler
# (startup/register.ps1). Logs to ~/.local/state/orphan-reaper/log.txt.
#
# A process counts only when its parent is gone (a parent id reused by a
# newer process counts as gone) and it is at least 5 minutes old:
# - suspended (every thread suspended) for 30 minutes or more, and one of
#   the tools below or git: stopped
# - busy (30 s of CPU in each of the last two 5-minute runs), and one of
#   the tools below (not git, whose own background work runs on after it
#   returns): stopped
# - busy and anything else: logged once, left running
# -DryRun logs what it would do and stops nothing. The other parameters
# change the limits and the state folder, for trying it out.
param(
    [switch]$DryRun,
    [double]$MinAgeMinutes = 5,
    [double]$SuspendedMinutes = 30,
    [double]$BusyCpuSeconds = 30,
    [int]$BusyRunsToStop = 2,
    [string]$StateDir = "$HOME\.local\state\orphan-reaper"
)

. "$PSScriptRoot\..\pwsh\aliases.ps1"

$StatePath = "$StateDir\cpu.json"
$LogPath = "$StateDir\log.txt"
$LogKeepLines = 2000

# Git Bash's tools as Claude Code's hooks and commands start them, and the
# statusline. Matched by name: a suspended process often has no path.
$Tools = @(
    'bash.exe', 'sh.exe', 'jq.exe', 'cat.exe', 'grep.exe', 'sed.exe', 'awk.exe', 'gawk.exe',
    'tr.exe', 'tail.exe', 'head.exe', 'cut.exe', 'sort.exe', 'uniq.exe', 'wc.exe', 'xargs.exe',
    'sha1sum.exe', 'cygpath.exe', 'dirname.exe', 'basename.exe', 'mkdir.exe', 'date.exe',
    'cygwin-console-helper.exe', 'claude-statusline.exe'
)

[void](ni -I Directory $StateDir -Force)

function Write-Log([string]$line) {
    "$((Get-Date).ToString('yyyy-MM-dd HH:mm:ss')) $line" >> $LogPath
}

$previous = @{}
if (tp $StatePath) {
    try { (gc $StatePath -Raw | ConvertFrom-Json -AsHashtable).GetEnumerator() | % { $previous[$_.Key] = $_.Value } } catch {}
}

$now = Get-Date
$all = Get-CimInstance Win32_Process
$created = @{}
foreach ($p in $all) { $created[[int]$p.ProcessId] = $p.CreationDate }

$state = @{}
foreach ($p in $all) {
    $id = [int]$p.ProcessId
    if ($id -le 4 -or !$p.CreationDate) { continue }
    $ageMinutes = ($now - $p.CreationDate).TotalMinutes
    if ($ageMinutes -lt $MinAgeMinutes) { continue }
    $parentCreated = $created[[int]$p.ParentProcessId]
    if ($parentCreated -and $parentCreated -le $p.CreationDate) { continue }

    $proc = Get-Process -Id $id -ea 0
    if (!$proc) { continue }
    $key = "$id|$($p.CreationDate.Ticks)"
    $cpu = $proc.TotalProcessorTime.TotalSeconds
    $last = $previous[$key]
    $busyRuns = 0
    if ($last -and ($cpu - $last.cpu) -ge $BusyCpuSeconds) { $busyRuns = $last.busy + 1 }
    $reported = $last ? [bool]$last.reported : $false
    $waits = @($proc.Threads | % { "$($_.WaitReason)" } | select -Unique)
    $suspended = $waits.Count -eq 1 -and $waits[0] -eq 'Suspended'
    $isTool = $Tools -contains $p.Name.ToLower()
    $what = "$($p.Name) pid=$id age=$([int]$ageMinutes)m cpu=$([int]$cpu)s cmd=$(($p.CommandLine ?? '') -replace '\s+', ' ')"

    $stop = $null
    if ($suspended -and $ageMinutes -ge $SuspendedMinutes -and ($isTool -or $p.Name -eq 'git.exe')) {
        $stop = 'suspended'
    } elseif ($busyRuns -ge $BusyRunsToStop -and $isTool) {
        $stop = 'busy'
    } elseif ($busyRuns -ge $BusyRunsToStop -and !$reported) {
        Write-Log "busy, left running: $what"
        $reported = $true
    }

    if ($stop) {
        if ($DryRun) {
            Write-Log "would stop ($stop): $what"
        } else {
            Stop-Process -Id $id -Force -ea 0
            Write-Log "stopped ($stop): $what"
        }
        if (!$DryRun) { continue }
    }
    $state[$key] = @{ cpu = $cpu; busy = $busyRuns; reported = $reported }
}

$state | ConvertTo-Json -Compress > $StatePath
if ((tp $LogPath) -and @(gc $LogPath).Count -gt $LogKeepLines) {
    $kept = gc $LogPath -Tail $LogKeepLines
    $kept > $LogPath
}
