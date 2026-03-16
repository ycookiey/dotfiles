#!/usr/bin/env pwsh
# doctor — dotfiles bootstrap 問題の原因調査
# Usage: doctor

$Dot = Split-Path $PSScriptRoot
$pass = 0
$fail = 0

function Ok($msg)   { Write-Host "  [PASS] $msg" -ForegroundColor Green; $script:pass++ }
function Ng($msg)   { Write-Host "  [FAIL] $msg" -ForegroundColor Red;   $script:fail++ }
function Info($msg)  { Write-Host "  [INFO] $msg" -ForegroundColor DarkGray }
function Section($t) { Write-Host "`n=== $t ===" -ForegroundColor Cyan }

# --------------------------------------------------
Section "Environment"
# --------------------------------------------------
Info "PS Version : $($PSVersionTable.PSVersion)"
Info "OS         : $([Environment]::OSVersion.VersionString)"
Info "dotfiles   : $Dot"

# --------------------------------------------------
Section "`$alias: syntax under ErrorActionPreference=Stop"
# --------------------------------------------------
# bootstrap/install スクリプトは $ErrorActionPreference="Stop" で aliases.ps1 を
# dot-source する。$alias: 構文がこの条件で失敗するかテスト。

$result = pwsh -NoProfile -NonInteractive -Command {
    $ErrorActionPreference = "Stop"
    try {
        $alias:tp = 'Test-Path'
        "OK"
    } catch {
        "ERR:$_"
    }
} 2>&1 | Out-String
$result = $result.Trim()

if ($result -eq "OK") {
    Ok "`$alias: 構文は Stop モードでもエラーなし（この環境では再現しない）"
    Info "既存エイリアスとの競合がないか確認してください"
} else {
    Ng "`$alias: 構文が Stop モードで失敗: $result"
    Info "→ これが wh not recognized の原因です"
}

# 既存エイリアス競合チェック
$conflicts = pwsh -NoProfile -NonInteractive -Command {
    $names = @('tp','sc')
    foreach ($n in $names) {
        $a = Get-Alias -Name $n -ea 0
        if ($a) { "$n -> $($a.Definition) (ReadOnly=$($a.Options -band [Management.Automation.ScopedItemOptions]::ReadOnly))" }
    }
} 2>&1 | Out-String
$conflicts = $conflicts.Trim()
if ($conflicts) {
    Ng "既存エイリアスが競合: $conflicts"
} else {
    Ok "tp/sc に既存エイリアスの競合なし"
}

# --------------------------------------------------
Section "aliases.ps1 dot-source テスト (Stop モード)"
# --------------------------------------------------
$aliasFile = "$Dot\pwsh\aliases.ps1"
if (!(Test-Path $aliasFile)) {
    $aliasFile = "$Dot/pwsh/aliases.ps1"
}

$helpers = pwsh -NoProfile -NonInteractive -Command "
    `$ErrorActionPreference = 'Stop'
    try {
        . '$aliasFile'
        `$ok = @()
        `$ng = @()
        foreach (`$n in @('wh','mkd','mkl','isadmin')) {
            if (Get-Command `$n -ea 0) { `$ok += `$n } else { `$ng += `$n }
        }
        foreach (`$n in @('tp','sc')) {
            if (Get-Alias `$n -ea 0) { `$ok += `$n } else { `$ng += `$n }
        }
        `"OK:`$(`$ok -join ',')|NG:`$(`$ng -join ',')`"
    } catch {
        `"ERR:`$_`"
    }
" 2>&1 | Out-String
$helpers = $helpers.Trim()

if ($helpers -match '^OK:') {
    $parts = $helpers -replace '^OK:' -split '\|NG:'
    $okList = $parts[0]
    $ngList = $parts[1]
    if ($ngList) {
        Ng "aliases.ps1 dot-source 後に未定義: $ngList"
    } else {
        Ok "aliases.ps1 の全ヘルパーが定義済み: $okList"
    }
} else {
    Ng "aliases.ps1 の dot-source が失敗: $helpers"
    Info "→ aliases.ps1 の Set-Alias 修正が必要です"
}

# --------------------------------------------------
Section "bootstrap 依存コマンド"
# --------------------------------------------------
$cmds = @(
    @{ Name = 'pwsh';  Req = $true  }
    @{ Name = 'git';   Req = $true  }
    @{ Name = 'scoop'; Req = $true  }
    @{ Name = 'cargo'; Req = $false }
)
foreach ($c in $cmds) {
    $found = Get-Command $c.Name -ea 0
    if ($found) {
        Ok "$($c.Name) が利用可能"
    } elseif ($c.Req) {
        Ng "$($c.Name) が見つかりません（bootstrap に必須）"
    } else {
        Info "$($c.Name) が見つかりません（オプション）"
    }
}

# --------------------------------------------------
Section "結果サマリー"
# --------------------------------------------------
Write-Host ""
if ($fail -eq 0) {
    Write-Host "  All $pass checks passed." -ForegroundColor Green
} else {
    Write-Host "  $pass passed, $fail failed." -ForegroundColor Yellow
}
