# Claude Code statusline: 2-line, 3-column (Model/Acc, 5h/7d usage + elapsed)
$claudeDir = $env:CLAUDE_CONFIG_DIR ? $env:CLAUDE_CONFIG_DIR : "$HOME\.claude"
. "$claudeDir\aliases.ps1"

function Bar([int]$pct, [int]$w = 6) {
    $f = [math]::Floor($pct * $w / 100)
    if ($pct -gt 0 -and $f -eq 0) { $f = 1 }
    $f = [math]::Max(0, [math]::Min($w, $f))
    ("▓" * $f).PadRight($w, "░")
}

function Stat([string]$label, [int]$pct) {
    $p = [math]::Max(0, [math]::Min(100, $pct))
    "{0}{1} {2,3}%" -f $label, (Bar $p), $p
}

# stdin: 非ブロッキング読み取り（200ms タイムアウト）
$j = $null
try {
    $task = [Console]::In.ReadLineAsync()
    if ($task.Wait(200)) { $j = $task.Result | ConvertFrom-Json -ea 0 }
} catch {}

$cacheFile = "$claudeDir\.statusline_cache"
$dirTail = [IO.Path]::GetFileName($claudeDir.TrimEnd([char[]]"/\"))
$acc = ($dirTail -match "(\d+)$") ? [int]$Matches[1] : 0

# --- Fetch API (120s TTL cache) ---
$stale = !(tp $cacheFile) -or ((Get-Date) - (gi $cacheFile -ea 0).LastWriteTime).TotalSeconds -ge 120
if ($stale) {
    try {
        $token = (gc "$claudeDir\.credentials.json" -Raw -ea 0 | ConvertFrom-Json -ea 0).claudeAiOauth.accessToken
        if ($token) {
            $h = @{ Authorization = "Bearer $token"; "anthropic-beta" = "oauth-2025-04-20" }
            @{ usage = irm "https://api.anthropic.com/api/oauth/usage" -Headers $h -TimeoutSec 3 -ea 0 } |
                ConvertTo-Json -Depth 10 | sc $cacheFile -ea 0
        }
    } catch {}
}

# --- Parse cache ---
$usage5hPct = 0; $elapsed5hPct = 0; $usage7dPct = 0; $elapsed7dPct = 0
try {
    if ((tp $cacheFile) -and ($cache = gc $cacheFile -Raw -ea 0 | ConvertFrom-Json -ea 0)) {
        $usage5hPct = [math]::Floor($cache.usage.five_hour.utilization)
        if ($reset5h = $cache.usage.five_hour.resets_at) {
            $remaining5h = [math]::Max(0, ([DateTimeOffset]::Parse($reset5h) - [DateTimeOffset]::UtcNow).TotalSeconds)
            $elapsed5hPct = [math]::Floor((18000 - $remaining5h) * 100 / 18000)
        }

        $usage7dPct = [math]::Floor($cache.usage.seven_day.utilization)
        if ($reset7d = $cache.usage.seven_day.resets_at) {
            $remaining7d = [math]::Max(0, ([DateTimeOffset]::Parse($reset7d) - [DateTimeOffset]::UtcNow).TotalSeconds)
            $elapsed7dPct = [math]::Floor((604800 - $remaining7d) * 100 / 604800)
        }
    }
} catch {}

# --- Output (2-line, 3-column aligned) ---
$modelName = $j ? $j.model.display_name : "?"
$cxStat = $j ? (Stat "Cx" ($j.context_window.used_percentage -as [int])) : "Cx------   ?%"
$colL = @("$modelName Acc:$acc", $cxStat)
$colC = @((Stat "5h" $usage5hPct), (Stat "5t" $elapsed5hPct))
$colR = @((Stat "7d" $usage7dPct), (Stat "7t" $elapsed7dPct))

$padL = ($colL | % Length | measure -Max).Maximum
$padC = ($colC | % Length | measure -Max).Maximum
$padR = ($colR | % Length | measure -Max).Maximum

0..1 | % {
    Write-Host "$($colL[$_].PadRight($padL))   $($colC[$_].PadRight($padC))   $($colR[$_].PadRight($padR))" -No:($_ -eq 1)
}
