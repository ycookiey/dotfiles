# Claude Code statusline: Model, Account, Context, 5h usage/elapsed (2-line)

function Bar([int]$pct, [int]$w = 10) {
    $f = [math]::Max(0, [math]::Min($w, [math]::Floor($pct * $w / 100)))
    "[" + ("▓" * $f).PadRight($w, "░") + "]"
}

function Stat([string]$label, [int]$pct) {
    $p = [math]::Max(0, [math]::Min(100, $pct))
    "{0}: {1} {2,3}%" -f $label, (Bar $p), $p
}

$j = [Console]::In.ReadToEnd() | ConvertFrom-Json
$claudeDir = "$HOME\.claude"
$cacheFile = "$claudeDir\.statusline_cache"

# --- Fetch API (120s TTL cache) ---
$stale = -not (Test-Path $cacheFile) -or ((Get-Date) - (Get-Item $cacheFile).LastWriteTime).TotalSeconds -ge 120
if ($stale) {
    try {
        $token = (Get-Content "$claudeDir\.credentials.json" -Raw | ConvertFrom-Json).claudeAiOauth.accessToken
        if ($token) {
            $h = @{ Authorization = "Bearer $token"; "anthropic-beta" = "oauth-2025-04-20" }
            $base = "https://api.anthropic.com/api/oauth"
            @{
                usage = Invoke-RestMethod "$base/usage" -Headers $h -TimeoutSec 3
                roles = Invoke-RestMethod "$base/claude_cli/roles" -Headers $h -TimeoutSec 3
            } | ConvertTo-Json -Depth 10 | Set-Content $cacheFile
        }
    } catch {}
}

# --- Parse cache ---
$usagePct = 0; $elapsedPct = 0; $account = "?"
try {
    if ((Test-Path $cacheFile) -and ($cache = Get-Content $cacheFile -Raw | ConvertFrom-Json)) {
        $usagePct = [math]::Floor($cache.usage.five_hour.utilization)
        if ($reset = $cache.usage.five_hour.resets_at) {
            $remaining = [math]::Max(0, ([DateTimeOffset]::Parse($reset) - [DateTimeOffset]::UtcNow).TotalSeconds)
            $elapsedPct = [math]::Floor((18000 - $remaining) * 100 / 18000)
        }
        if ($cache.roles.organization_name -match "^([^@]+@[^']+)'s Organization$") {
            $account = $Matches[1][0] + "**"
        }
    }
} catch {}

# --- Output (2-line, aligned) ---
$left = @("[$($j.model.display_name)] Acc: $account", (Stat "Ctx" ($j.context_window.used_percentage -as [int])))
$right = @((Stat "Used" $usagePct), (Stat "Time" $elapsedPct))
$pad = ($left | ForEach-Object { $_.Length } | Measure-Object -Maximum).Maximum
0..1 | ForEach-Object { Write-Host "$($left[$_].PadRight($pad))   $($right[$_])" -NoNewline:($_ -eq 1) }
