#!/usr/bin/env pwsh
# ycusage — 全アカウントのClaude Code使用状況一覧（statuslineキャッシュ版）
. "$PSScriptRoot\..\pwsh\aliases.ps1"

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
    try {
        $v = if ($resetsAt -is [long] -or $resetsAt -is [int] -or $resetsAt -is [double]) {
            [DateTimeOffset]::FromUnixTimeSeconds([long]$resetsAt)
        } else {
            [DateTimeOffset]::Parse([string]$resetsAt)
        }
        [Math]::Max(0, ($v - [DateTimeOffset]::UtcNow).TotalSeconds)
    } catch { $null }
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

# statusline キャッシュ (.rate-limits.json) から usage を読み取る
# resets_at が過去ならウィンドウはリセット済み → utilization = 0
function Get-UsageFromCache([string]$credDir) {
    $cacheFile = "$credDir\.rate-limits.json"
    if (!(tp $cacheFile)) { return @{ Err = $true; Reason = 'no_cache' } }
    $c = gc $cacheFile -Raw -ea 0 | ConvertFrom-Json -ea 0
    if (!$c -or !$c.rate_limits) { return @{ Err = $true; Reason = 'no_cache' } }
    $rl = $c.rate_limits
    $now = [DateTimeOffset]::UtcNow

    # 5h window
    $r5 = Get-RemainingSeconds $rl.five_hour.resets_at
    $p5raw = [double]($rl.five_hour.used_percentage ?? 0)
    # resets_at が過去 → リセット済み
    $p5 = ($null -ne $r5 -and $r5 -le 0) ? 0 : [int][Math]::Floor($p5raw)

    # 7d window
    $r7 = Get-RemainingSeconds $rl.seven_day.resets_at
    $p7raw = [double]($rl.seven_day.used_percentage ?? 0)
    $p7 = ($null -ne $r7 -and $r7 -le 0) ? 0 : [int][Math]::Floor($p7raw)

    # resets_at が過去なら remaining = null（表示上 "-"）
    $r5disp = ($null -ne $r5 -and $r5 -gt 0) ? $r5 : $null
    $r7disp = ($null -ne $r7 -and $r7 -gt 0) ? $r7 : $null

    @{
        Err = $false
        P5  = $p5
        E5  = $null -ne $r5disp ? (Format-Elapsed 18000 $r5disp) : 0
        L5  = Format-5hLeft $r5disp
        R5  = $r5disp
        P7  = $p7
        E7  = $null -ne $r7disp ? (Format-Elapsed 604800 $r7disp) : 0
        L7  = Format-7dLeft $r7disp
        R7  = $r7disp
        CachedAt = [long]($c.cached_at ?? 0)
    }
}

# --- Main ---
$accDirs = gci "$HOME\.claude-*" -Directory -ea 0 |
    ? { $_.Name -match "^\.claude-(\d+)$" }

$accounts = @($accDirs | % {
    $n = [int]($_.Name -replace '^\.claude-', '')
    $dir = $_.FullName

    # サブスク無し判定
    if (tp "$dir\.no-subscription") {
        return @{ N = $n; U = @{ Err = $true; Reason = 'no_subscription' } }
    }
    $credFile = "$dir\.credentials.json"
    if (!(tp $credFile)) {
        return @{ N = $n; U = @{ Err = $true; Reason = 'no_token' } }
    }
    $oauth = (gc $credFile -Raw -ea 0 | ConvertFrom-Json -ea 0).claudeAiOauth
    if (!$oauth -or !$oauth.subscriptionType) {
        return @{ N = $n; U = @{ Err = $true; Reason = 'no_subscription' } }
    }

    # キャッシュから読み取り
    $u = Get-UsageFromCache $dir
    @{ N = $n; U = $u }
} | sort { $_.N })

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
        } elseif ($u.Reason -eq 'no_cache') {
            wh "  --  (no data - use this account once to populate)" -ForegroundColor DarkGray
        } else {
            wh "  --  (auth failed - run " -NoNewline -ForegroundColor DarkGray
            wh "c $n" -ForegroundColor Cyan -NoNewline
            wh " to re-auth)" -ForegroundColor DarkGray
        }
        continue
    }

    # キャッシュ鮮度表示
    $stale = ""
    if ($u.CachedAt) {
        $ago = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() - $u.CachedAt
        if ($ago -gt 3600) {
            $hAgo = [int][Math]::Floor($ago / 3600)
            $stale = " ({0}h ago)" -f $hAgo
        }
    }

    $l5 = "$($u.L5)"; $l7 = "$($u.L7)"
    $timeW = 7
    $pad = " " * ($l5.PadRight($timeW).Length + 2)

    wh "  " -NoNewline; wh "5h " -ForegroundColor $lc -NoNewline
    wh "$(Stat $u.P5)$pad" -NoNewline; wh $sep -NoNewline
    wh "7d " -ForegroundColor $lc -NoNewline; wh "$(Stat $u.P7)" -NoNewline
    if ($stale) { wh $stale -ForegroundColor DarkGray } else { wh "" }

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
