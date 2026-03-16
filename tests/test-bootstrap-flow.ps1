# test-bootstrap-flow.ps1 — bootstrap の実際のフローを再現して wh スコープ問題を検証
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path (Split-Path $MyInvocation.MyCommand.Definition)
$failed = 0

function Test-Pass($msg) { Write-Host "  PASS: $msg" -ForegroundColor Green }
function Test-Fail($msg) { Write-Host "  FAIL: $msg" -ForegroundColor Red; $script:failed++ }
function Test-Info($msg) { Write-Host "  INFO: $msg" -ForegroundColor Yellow }

# === Test 1: 同一プロセス内での & 呼び出し（setup.ps1 → scoop.ps1 のシミュレート） ===
Write-Host "`n[Test 1] Same-process: dot-source aliases then & child script" -ForegroundColor Cyan

$childScript = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '.ps1'
@'
$ErrorActionPreference = "Stop"
try {
    $null = Get-Command wh -ErrorAction Stop
    Write-Output "FOUND"
} catch {
    Write-Output "NOT_FOUND"
}
'@ | Set-Content $childScript

. "$ScriptDir\pwsh\aliases.ps1"
$result = & $childScript
Remove-Item $childScript -Force

if ($result -eq "FOUND") {
    Test-Pass "wh visible in same-process child scope via &"
} else {
    Test-Fail "wh NOT visible in same-process child scope via &"
}

# === Test 2: 別プロセス (pwsh -File) — bootstrap.ps1 が実際にやっていること ===
Write-Host "`n[Test 2] Separate process: pwsh -File setup.ps1 (actual bootstrap flow)" -ForegroundColor Cyan

$setupSim = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '.ps1'
@"
`$ErrorActionPreference = "Stop"
. "$ScriptDir\pwsh\aliases.ps1"
try {
    `$null = Get-Command wh -ErrorAction Stop
    Write-Output "SETUP_HAS_WH"
} catch {
    Write-Output "SETUP_NO_WH"
}

# scoop.ps1 と同じように子スクリプトを & で呼ぶ
`$child = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '.ps1'
@'
`$ErrorActionPreference = "Stop"
try {
    `$null = Get-Command wh -ErrorAction Stop
    Write-Output "CHILD_HAS_WH"
} catch {
    Write-Output "CHILD_NO_WH"
}
'@ | Set-Content `$child
& `$child
Remove-Item `$child -Force
"@ | Set-Content $setupSim

$pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
if ($pwsh) {
    $output = & pwsh -ExecutionPolicy Bypass -File $setupSim
    Remove-Item $setupSim -Force

    foreach ($line in $output) {
        switch ($line) {
            "SETUP_HAS_WH"  { Test-Pass "setup.ps1 scope has wh after dot-source" }
            "SETUP_NO_WH"   { Test-Fail "setup.ps1 scope does NOT have wh after dot-source" }
            "CHILD_HAS_WH"  { Test-Pass "child script (scoop.ps1 equivalent) can see wh" }
            "CHILD_NO_WH"   { Test-Fail "child script (scoop.ps1 equivalent) CANNOT see wh" }
        }
    }
} else {
    Test-Info "pwsh not found, skipping separate-process test"
}

# === Test 3: scoop.ps1 の静的解析 — aliases.ps1 の import 有無 ===
Write-Host "`n[Test 3] Static analysis: does scoop.ps1 import aliases.ps1?" -ForegroundColor Cyan

$scoopContent = Get-Content "$ScriptDir\install\scoop.ps1" -Raw
$customCmds = @('wh')
$usesCustom = $customCmds | Where-Object { $scoopContent -match "\b$_\b" }
$importsAliases = $scoopContent -match '\.\s+.*aliases\.ps1'

if ($usesCustom -and -not $importsAliases) {
    Test-Fail "scoop.ps1 uses custom commands ($($usesCustom -join ', ')) but does NOT import aliases.ps1"
    Write-Host "        -> This is the root cause of the bootstrap failure" -ForegroundColor Yellow
} elseif ($usesCustom -and $importsAliases) {
    Test-Pass "scoop.ps1 uses custom commands and imports aliases.ps1"
} else {
    Test-Pass "scoop.ps1 does not use custom commands"
}

# === Test 4: install/ 配下の他のスクリプトも同じ問題がないか ===
Write-Host "`n[Test 4] Check all scripts under install/ for unresolved alias deps" -ForegroundColor Cyan

$scripts = Get-ChildItem "$ScriptDir\install\*.ps1" -ErrorAction SilentlyContinue
foreach ($s in $scripts) {
    $content = Get-Content $s.FullName -Raw
    $uses = $customCmds | Where-Object { $content -match "\b$_\b" }
    $imports = $content -match '\.\s+.*aliases\.ps1'
    if ($uses -and -not $imports) {
        Test-Fail "$($s.Name) uses ($($uses -join ', ')) without importing aliases.ps1"
    } elseif ($uses) {
        Test-Pass "$($s.Name) uses custom commands and imports aliases.ps1"
    } else {
        Test-Pass "$($s.Name) does not use custom commands"
    }
}

# === 結果 ===
Write-Host ""
if ($failed -eq 0) {
    Write-Host "All tests passed!" -ForegroundColor Green
} else {
    Write-Host "$failed test(s) FAILED" -ForegroundColor Red
    exit 1
}
