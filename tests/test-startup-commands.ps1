# test-startup-commands.ps1 — checks that every app startup/manager.ps1 starts resolves to a command
#
# manager.ps1 runs each launcher inside Start-App-Logged's try/catch, so a renamed or removed
# command is only visible as an ERROR line in logs/startup-*.log. This test takes the first command
# of each Start-App-Logged scriptblock (and the bare-word target of `start <name>`) and resolves it
# with Get-Command after loading what manager.ps1 loads (pwsh/aliases.ps1, generated-aliases.ps1).
#
# generated-aliases.ps1 is a gitignored build output: run `dotcli generate` first, or pass a file
# generated elsewhere with -GeneratedAliases.
param(
    [string]$ManagerPath,
    [string]$GeneratedAliases
)
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path (Split-Path $MyInvocation.MyCommand.Definition)
if (!$ManagerPath) { $ManagerPath = "$ScriptDir\startup\manager.ps1" }
if (!$GeneratedAliases) { $GeneratedAliases = "$ScriptDir\generated-aliases.ps1" }
$failed = 0

if (![IO.File]::Exists($GeneratedAliases)) {
    Write-Host "FAIL: $GeneratedAliases not found (run dotcli generate first)" -ForegroundColor Red
    exit 1
}

# Same order as manager.ps1: generated launchers such as ncal call Start-App from aliases.ps1.
. "$ScriptDir\pwsh\aliases.ps1"
. $GeneratedAliases

$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($ManagerPath, [ref]$null, [ref]$parseErrors)
if ($parseErrors) {
    Write-Host "FAIL: $ManagerPath has parse errors" -ForegroundColor Red
    $parseErrors | % { Write-Host "  $($_.Extent.StartLineNumber): $($_.Message)" -ForegroundColor Red }
    exit 1
}

$calls = @($ast.FindAll({
    param($n)
    $n -is [System.Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq 'Start-App-Logged'
}, $true))
# Guards against a rename of Start-App-Logged turning this test into a no-op.
if ($calls.Count -eq 0) {
    Write-Host "FAIL: no Start-App-Logged calls found in $ManagerPath" -ForegroundColor Red
    exit 1
}

Write-Host "`n[Test] Commands started by $ManagerPath" -ForegroundColor Cyan
foreach ($call in $calls) {
    $line = $call.Extent.StartLineNumber
    $app = $call.CommandElements[1].Extent.Text
    $block = $call.CommandElements | ? { $_ -is [System.Management.Automation.Language.ScriptBlockExpressionAst] } | select -First 1
    $first = if ($block) {
        $block.ScriptBlock.FindAll({ $args[0] -is [System.Management.Automation.Language.CommandAst] }, $true) |
            sort { $_.Extent.StartOffset } | select -First 1
    }
    $cmd = if ($first) { $first.GetCommandName() }
    if (!$cmd) {
        Write-Host "  FAIL: ${line}: $app has no command to check" -ForegroundColor Red
        $failed++
        continue
    }

    $names = @($cmd)
    # `start <name>` resolves <name> itself; paths and expandable strings are machine-specific.
    $resolved = Get-Command $cmd -ea 0
    $resolvedName = if ($resolved -is [System.Management.Automation.AliasInfo]) { $resolved.ResolvedCommandName } else { $resolved.Name }
    if ($resolvedName -eq 'Start-Process') {
        $target = $first.CommandElements[1]
        if ($target -is [System.Management.Automation.Language.StringConstantExpressionAst] -and
            $target.StringConstantType -eq 'BareWord') {
            $names += $target.Value
        }
    }

    foreach ($name in $names) {
        if (Get-Command $name -ea 0) {
            Write-Host "  PASS: ${line}: $app -> '$name'" -ForegroundColor Green
        } else {
            Write-Host "  FAIL: ${line}: $app -> '$name' is MISSING" -ForegroundColor Red
            $failed++
        }
    }
}

# --- Result ---
Write-Host ""
if ($failed -eq 0) {
    Write-Host "All tests passed!" -ForegroundColor Green
} else {
    Write-Host "$failed test(s) FAILED" -ForegroundColor Red
    exit 1
}
