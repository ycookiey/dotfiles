# test-scoop-aliases.ps1 — scoop.ps1 が依存する関数の存在を検証
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path (Split-Path $MyInvocation.MyCommand.Definition)
$failed = 0

function Assert-CommandExists($name, $source) {
    if (Get-Command $name -ErrorAction SilentlyContinue) {
        Write-Host "  PASS: '$name' is defined" -ForegroundColor Green
    } else {
        Write-Host "  FAIL: '$name' is NOT defined (expected from $source)" -ForegroundColor Red
        $script:failed++
    }
}

# --- Test 1: scoop.ps1 を直接実行したとき wh が使えるか ---
Write-Host "`n[Test 1] Check if scoop.ps1 defines/imports its dependencies" -ForegroundColor Cyan

# scoop.ps1 の先頭部分だけを解析して、aliases.ps1 を dot-source しているか確認
$scoopScript = Get-Content "$ScriptDir\install\scoop.ps1" -Raw
$importAliases = $scoopScript -match '\.\s+.*aliases\.ps1'

if ($importAliases) {
    Write-Host "  PASS: scoop.ps1 imports aliases.ps1" -ForegroundColor Green
} else {
    Write-Host "  FAIL: scoop.ps1 does NOT import aliases.ps1" -ForegroundColor Red
    Write-Host "        'wh' function will be undefined when scoop.ps1 runs" -ForegroundColor Yellow
    $failed++
}

# --- Test 2: setup.ps1 から & で呼ぶとスコープが引き継がれないことを確認 ---
Write-Host "`n[Test 2] Verify & (call operator) does NOT inherit parent functions" -ForegroundColor Cyan

$testChild = @'
if (Get-Command wh -ErrorAction SilentlyContinue) {
    Write-Output "INHERITED"
} else {
    Write-Output "NOT_INHERITED"
}
'@
$tmpFile = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '.ps1'
$testChild | Set-Content $tmpFile

# 親スコープで wh を定義
function wh { Write-Host @args }
$result = & $tmpFile
Remove-Item $tmpFile -Force

if ($result -eq "NOT_INHERITED") {
    Write-Host "  PASS: & operator correctly isolates scope (wh not inherited)" -ForegroundColor Green
    Write-Host "        This confirms scoop.ps1 needs its own import" -ForegroundColor Yellow
} else {
    Write-Host "  INFO: & operator inherited parent functions (unexpected)" -ForegroundColor Yellow
}

# --- Test 3: aliases.ps1 が必要な関数をすべて定義しているか ---
Write-Host "`n[Test 3] Verify aliases.ps1 defines required functions" -ForegroundColor Cyan
. "$ScriptDir\pwsh\aliases.ps1"
Assert-CommandExists "wh" "aliases.ps1"
Assert-CommandExists "mkd" "aliases.ps1"
Assert-CommandExists "mkl" "aliases.ps1"
Assert-CommandExists "isadmin" "aliases.ps1"

# --- Test 4: scoop.ps1 内で使われるエイリアスをすべてチェック ---
Write-Host "`n[Test 4] Check all custom aliases/functions used in scoop.ps1" -ForegroundColor Cyan

$usedFunctions = @('wh', 'gc', 'gcm')  # gc, gcm are built-in aliases
foreach ($fn in $usedFunctions) {
    Assert-CommandExists $fn "scoop.ps1 dependency"
}

# --- 結果 ---
Write-Host ""
if ($failed -eq 0) {
    Write-Host "All tests passed!" -ForegroundColor Green
} else {
    Write-Host "$failed test(s) FAILED" -ForegroundColor Red
    exit 1
}
