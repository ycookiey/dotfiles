#!/usr/bin/env pwsh
# ycusage — 全アカウントのClaude Code使用状況一覧
. "$PSScriptRoot\..\pwsh\aliases.ps1"

$script:LogFile = "$HOME\.claude\ycusage.log"

function Log($msg) { $msg | ac "$HOME\.claude\ycusage.log" -Enc utf8 }

function Bar([int]$pct, [int]$w = 10) {
    $f = [Math]::Floor($pct * $w / 100)
    if ($pct -gt 0 -and $f -eq 0) { $f = 1 }
    $f = [Math]::Max(0, [Math]::Min($w, $f))
    "[" + ("▓" * $f).PadRight($w, "░") + "]"
}

function Stat([int]$pct) {
    $p = [Math]::Max(0, [Math]::Min(100, $pct))
    "{0} {1,3}%" -f (Bar $p), $p
}

function Get-RemainingSeconds([object]$resetsAt) {
    if (!$resetsAt) { return $null }
    try { [Math]::Max(0, ([DateTimeOffset]::Parse([string]$resetsAt) - [DateTimeOffset]::UtcNow).TotalSeconds) } catch { $null }
}

function Format-Elapsed([double]$total, [double]$remaining) {
    [int][Math]::Floor([Math]::Max(0, $total - $remaining) * 100 / $total)
}

function Format-5hLeft($remaining) {
    if ($null -eq $remaining) { return "-" }
    $m = [int][Math]::Floor([Math]::Max(0, $remaining) / 60)
    "{0}h{1:00}m" -f [Math]::Floor($m / 60), ($m % 60)
}

function Format-7dLeft($remaining) {
    if ($null -eq $remaining) { return "-" }
    $s = [Math]::Max(0, $remaining)
    $d = [int][Math]::Floor($s / 86400); $h = [int][Math]::Floor(($s % 86400) / 3600)
    if ($d -gt 0) { "{0}d{1}h" -f $d, $h }
    elseif ($h -gt 0) { "{0}h" -f $h }
    else { "{0}m" -f [int][Math]::Floor(($s % 3600) / 60) }
}

# 余裕度(経過率-使用率)によるアカウント推奨
# 7d優先→5hタイブレーク。正>零>負、負は|余裕度|最大。
function Compare-Window($a, $b, [string]$hK, [string]$uK, [string]$eK, [bool]$is5h) {
    $hA = $a[$hK]; $hB = $b[$hK]
    if ($null -eq $hA -and $null -eq $hB) { return 0 }
    if ($null -eq $hA) { return 1 }
    if ($null -eq $hB) { return -1 }
    $sA = [Math]::Sign([Math]::Round($hA, 6))
    $sB = [Math]::Sign([Math]::Round($hB, 6))
    if ($sA -ne $sB) { return $sA.CompareTo($sB) }
    if ($sA -gt 0) {
        $c = $hA.CompareTo($hB); if ($c -ne 0) { return $c }
        return $b.U[$uK].CompareTo($a.U[$uK])
    }
    if ($sA -eq 0) { return $b.U[$uK].CompareTo($a.U[$uK]) }
    $c = [Math]::Abs($hA).CompareTo([Math]::Abs($hB)); if ($c -ne 0) { return $c }
    $is5h ? $a.U[$eK].CompareTo($b.U[$eK]) : $b.U[$uK].CompareTo($a.U[$uK])
}

function Get-Recommended($accs) {
    $cands = @($accs | ? { !$_.U.Err -and $_.U.P5 -lt 95 -and $_.U.P7 -lt 95 })
    if (!$cands) { return $null }
    foreach ($c in $cands) {
        $c['H7'] = $null -eq $c.U.R7 ? $null : ((1 - [Math]::Max(0, $c.U.R7) / 604800) - $c.U.P7 / 100)
        $c['H5'] = $null -eq $c.U.R5 ? $null : ((1 - [Math]::Max(0, $c.U.R5) / 18000) - $c.U.P5 / 100)
    }
    # 通常ランキング: 7d余裕度→5h→番号
    $best = $cands[0]
    for ($i = 1; $i -lt $cands.Count; $i++) {
        $c = $cands[$i]
        $r = Compare-Window $c $best 'H7' 'P7' 'E7' $false
        if ($r -gt 0) { $best = $c; continue }
        if ($r -lt 0) { continue }
        $r = Compare-Window $c $best 'H5' 'P5' 'E5' $true
        if ($r -gt 0) { $best = $c; continue }
        if ($r -lt 0) { continue }
        if ($c.N -lt $best.N) { $best = $c }
    }
    # 消化モード: 余裕度が僅差(<=5pt)かつ7t>=80%のアカウントがあれば、7t最大を優先
    $bestH = $null -eq $best['H7'] ? 100.0 : ($best['H7'] * 100)
    $close = @($cands | ? {
        $h = $null -eq $_['H7'] ? 100.0 : ($_['H7'] * 100)
        ($bestH - $h) -le 5 -and ($bestH - $h) -ge 0
    })
    if ($close.Count -gt 1 -and ($close | ? { $_.U.E7 -ge 80 })) {
        return ($close | sort { -$_.U.E7 }, { $_.N } | select -First 1)
    }
    $best
}

function Ensure-FreshToken([string]$credFile) {
    $accNum = [int]([IO.Path]::GetFileName((Split-Path $credFile).TrimEnd([char[]]"/\")) -replace '^\.claude-', '')
    try {
        $cred = gc $credFile -Raw -ea 0 | ConvertFrom-Json -ea 0
        $oauth = $cred.claudeAiOauth
        if (!$oauth -or !$oauth.refreshToken) { Log "[Acc $accNum] No refreshToken"; return }
        $expiresIn = [long]$oauth.expiresAt - [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        if ($expiresIn -gt 300000) { Log "[Acc $accNum] Token fresh (${expiresIn}ms)"; return }
        Log "[Acc $accNum] Refreshing (${expiresIn}ms left)"
        $body = @{ grant_type = "refresh_token"; refresh_token = $oauth.refreshToken; client_id = "9d1c250a-e61b-44d9-88ed-5944d1962f5e" } | ConvertTo-Json
        $resp = irm "https://console.anthropic.com/v1/oauth/token" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 5 -NoProxy -ea Stop
        if (!$resp.access_token) { Log "[Acc $accNum] Refresh failed: no access_token"; return }
        $oauth.accessToken = $resp.access_token
        if ($resp.refresh_token) { $oauth.refreshToken = $resp.refresh_token }
        if ($resp.expires_in) { $oauth.expiresAt = [DateTimeOffset]::UtcNow.AddSeconds([long]$resp.expires_in).ToUnixTimeMilliseconds() }
        $cred | ConvertTo-Json -Depth 10 | Set-Content $credFile -Enc utf8NoBOM
        Log "[Acc $accNum] Token refreshed"
    } catch {
        Log "[Acc $accNum] Refresh error: $($_.Exception.Message)"
    }
}

function Get-Usage([string]$credFile) {
    $accNum = [int]([IO.Path]::GetFileName((Split-Path $credFile).TrimEnd([char[]]"/\")) -replace '^\.claude-', '')
    $oauth = (gc $credFile -Raw -ea 0 | ConvertFrom-Json -ea 0).claudeAiOauth
    if (!$oauth -or !$oauth.accessToken) { return @{ Err = $true; Reason = 'no_token' } }
    Ensure-FreshToken $credFile
    # Re-read after refresh (Ensure-FreshToken updates the file, not $oauth)
    $oauth = (gc $credFile -Raw -ea 0 | ConvertFrom-Json -ea 0).claudeAiOauth
    try {
        $h = @{ Authorization = "Bearer $($oauth.accessToken)"; "anthropic-beta" = "oauth-2025-04-20" }
        $u = irm "https://api.anthropic.com/api/oauth/usage" -Headers $h -TimeoutSec 8 -NoProxy -ea Stop
        Log "[Acc $accNum] OK"
        $r5 = Get-RemainingSeconds $u.five_hour.resets_at
        $r7 = Get-RemainingSeconds $u.seven_day.resets_at
        @{
            Err = $false
            P5  = [int][Math]::Floor([double]($u.five_hour.utilization ?? 0))
            E5  = $null -ne $r5 ? (Format-Elapsed 18000 $r5) : 0
            L5  = Format-5hLeft $r5
            R5  = $r5
            P7  = [int][Math]::Floor([double]($u.seven_day.utilization ?? 0))
            E7  = $null -ne $r7 ? (Format-Elapsed 604800 $r7) : 0
            L7  = Format-7dLeft $r7
            R7  = $r7
        }
    } catch {
        $msg = $_.Exception.Message
        Log "[Acc $accNum] Error: $msg"
        $reason = if ($msg -match 'subscription.expired|402') { 'no_subscription' }
                  elseif (!$oauth.subscriptionType) { 'no_subscription' }
                  else { 'auth' }
        @{ Err = $true; Reason = $reason }
    }
}

# --- Main ---
"[$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')] ycusage start" | Set-Content $script:LogFile -Enc utf8

$credFiles = gci "$HOME\.claude-*" -Directory -ea 0 |
    ? { $_.Name -match "^\.claude-(\d+)$" } |
    % { "$($_.FullName)\.credentials.json" } |
    ? { tp $_ }

# ForEach-Object -Parallel は関数スコープを引き継がないため文字列として渡す
$fnNames = 'Ensure-FreshToken','Get-RemainingSeconds','Format-Elapsed','Format-5hLeft','Format-7dLeft','Get-Usage','Log'
$fnStrs = @{}; $fnNames | % { $fnStrs[$_] = (gi "function:$_").ScriptBlock.ToString() }

$accounts = @($credFiles | ForEach-Object -Parallel {
    $fns = $using:fnStrs
    $fns.Keys | % { Set-Item "function:$_" ([scriptblock]::Create($fns[$_])) }
    $n = [int]([IO.Path]::GetFileName((Split-Path $_).TrimEnd([char[]]"/\")) -replace '^\.claude-', '')
    @{ N = $n; U = Get-Usage $_ }
} -ThrottleLimit 10 | sort { $_.N })

# --- Re-auth ---
$authFailed = @($accounts | ? { $_.U.Err -and $_.U.Reason -eq 'auth' })
if ($authFailed) {
    $names = ($authFailed | % { "Acc $($_.N)" }) -join ', '
    wh "Re-auth required: $names" -ForegroundColor DarkYellow
    foreach ($a in $authFailed) {
        $n = $a.N
        if (& "$PSScriptRoot\claude-reauth.ps1" -AccNum $n) {
            $a.U = Get-Usage "$HOME\.claude-$n\.credentials.json"
        }
    }
}

if (!$accounts) { wh "No accounts found."; return }

# --- Display ---
$activeAcc = $null
$la = gc "$HOME\.claude\.last_account" -Raw -ea 0
if ($la -and $la.Trim() -match '^\d+$') { $activeAcc = [int]$la.Trim() }

$sep = " │ "; $lc = "DarkCyan"

foreach ($a in $accounts) {
    $u = $a.U; $n = $a.N
    wh ""
    wh ($n -eq $activeAcc ? "Acc $n *" : "Acc $n") -ForegroundColor ($n -eq $activeAcc ? "Green" : "White")

    if ($u.Err) {
        if ($u.Reason -in 'subscription_expired', 'no_subscription') {
            wh "  --  (no subscription)" -ForegroundColor DarkGray
        } else {
            wh "  --  (auth failed - run " -NoNewline -ForegroundColor DarkGray
            wh "c $n" -ForegroundColor Cyan -NoNewline
            wh " to re-auth)" -ForegroundColor DarkGray
        }
        continue
    }

    $l5 = "$($u.L5)"; $l7 = "$($u.L7)"
    $timeW = 7
    $pad = " " * ($l5.PadRight($timeW).Length + 2)

    wh "  " -NoNewline; wh "5h " -ForegroundColor $lc -NoNewline
    wh "$(Stat $u.P5)$pad" -NoNewline; wh $sep -NoNewline
    wh "7d " -ForegroundColor $lc -NoNewline; wh (Stat $u.P7)

    wh "  " -NoNewline; wh "5t " -ForegroundColor $lc -NoNewline
    wh "$(Stat $u.E5)  " -NoNewline; wh $l5.PadRight($timeW) -ForegroundColor Cyan -NoNewline
    wh $sep -NoNewline; wh "7t " -ForegroundColor $lc -NoNewline
    wh "$(Stat $u.E7)  " -NoNewline; wh $l7 -ForegroundColor Cyan
}

# --- Recommend ---
$rec = Get-Recommended $accounts
if ($rec) {
    wh ""; wh ">>> Use account $($rec.N)" -ForegroundColor Yellow
    # 推奨番号とタイムスタンプを保存（c 引数なしで参照）
    "$($rec.N)`t$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())" | sc "$HOME\.claude\.recommended" -No
}
wh ""
