# Claude Code statusline: Model, Account, Context, 5h usage/elapsed (2-line)
$j = [Console]::In.ReadToEnd() | ConvertFrom-Json

function Bar($pct, $width = 10) {
    $pct = [math]::Max(0, [math]::Min(100, [int]$pct))
    $filled = [math]::Floor($pct * $width / 100)
    # 10% 未満は 0 個（差をわかりやすく）
    $empty = $width - $filled
    return "[" + ("▓" * $filled) + ("░" * $empty) + "]"
}

# --- Context ---
$ctxPct = if ($j.context_window.used_percentage) { $j.context_window.used_percentage } else { 0 }

# --- API calls with cache ---
$claudeDir = "$HOME\.claude"
$cacheFile = "$claudeDir\.statusline_cache"
$cacheTTL = 120
$usagePct = 0
$elapsedPct = 0
$account = "?"

$fetchApi = $true
if (Test-Path $cacheFile) {
    $age = ((Get-Date) - (Get-Item $cacheFile).LastWriteTime).TotalSeconds
    if ($age -lt $cacheTTL) { $fetchApi = $false }
}

if ($fetchApi) {
    try {
        $creds = Get-Content "$claudeDir\.credentials.json" -Raw | ConvertFrom-Json
        $token = $creds.claudeAiOauth.accessToken
        if ($token) {
            $headers = @{
                "Accept"         = "application/json"
                "Content-Type"   = "application/json"
                "Authorization"  = "Bearer $token"
                "anthropic-beta" = "oauth-2025-04-20"
            }
            # Fetch usage and roles in parallel would be nice, but keep it simple
            $usage = Invoke-RestMethod -Uri "https://api.anthropic.com/api/oauth/usage" -Headers $headers -TimeoutSec 3
            $roles = Invoke-RestMethod -Uri "https://api.anthropic.com/api/oauth/claude_cli/roles" -Headers $headers -TimeoutSec 3
            @{ usage = $usage; roles = $roles } | ConvertTo-Json -Depth 10 | Set-Content $cacheFile
        }
    } catch {}
}

if (Test-Path $cacheFile) {
    try {
        $cache = Get-Content $cacheFile -Raw | ConvertFrom-Json

        # Usage
        $usagePct = [math]::Floor($cache.usage.five_hour.utilization)
        $resetsAt = $cache.usage.five_hour.resets_at
        if ($resetsAt) {
            $resetTime = [DateTimeOffset]::Parse($resetsAt)
            $remaining = ($resetTime - [DateTimeOffset]::UtcNow).TotalSeconds
            if ($remaining -lt 0) { $remaining = 0 }
            $totalSec = 5 * 3600
            $elapsedPct = [math]::Floor(($totalSec - $remaining) * 100 / $totalSec)
            $elapsedPct = [math]::Max(0, [math]::Min(100, $elapsedPct))
        }

        # Account (extract email from "xxx@gmail.com's Organization")
        $orgName = $cache.roles.organization_name
        if ($orgName -match "^([^@]+@[^']+)'s Organization$") {
            $email = $Matches[1]
            # Anonymize: first char + ** (e.g., "y**")
            $account = $email.Substring(0, 1) + "**"
        }
    } catch {}
}

# 2-line output: line1=Model+Acc+Used, line2=Ctx+Time (Used/Time aligned)
$model = $j.model.display_name
$modelPart = "[$model] "
$accPart = "Acc: $account"
$ctxPart = "Ctx: $(Bar $ctxPct) {0,3}%" -f $ctxPct
$usedPart = "Used: $(Bar $usagePct) {0,3}%" -f $usagePct
$timePart = "Time: $(Bar $elapsedPct) {0,3}%" -f $elapsedPct

# Calculate padding to align Used and Time
$line1Left = "$modelPart$accPart"
$line2Left = $ctxPart
$maxLeft = [math]::Max($line1Left.Length, $line2Left.Length)

$line1 = $line1Left.PadRight($maxLeft) + "   $usedPart"
$line2 = $line2Left.PadRight($maxLeft) + "   $timePart"

Write-Host $line1
Write-Host -NoNewline $line2
