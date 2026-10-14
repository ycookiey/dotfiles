# Regression test for pwsh/profile.ps1 prompt: $? reaches starship, and a missing/empty starship cache does not recurse.
# Extracts $global:_promptHook and the outer `function global:prompt` from the profile via AST (never dot-sources the profile).
param([string]$Profile = (Join-Path $PSScriptRoot '..\pwsh\profile.ps1'))
$ErrorActionPreference = 'Stop'
$fail = 0
function Check($name, $ok) { if ($ok) { "ok   $name" } else { "FAIL $name"; $script:fail++ } }

$ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $Profile), [ref]$null, [ref]$null)
$hookAst = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$global:_promptHook' }, $true)
$promptAst = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'global:prompt' }, $true)
if (!$hookAst -or !$promptAst) { throw 'prompt/hook not found in profile' }
$promptText = $promptAst.Extent.Text

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("prompt-test-" + [guid]::NewGuid())
$null = New-Item -ItemType Directory $tmp

function Reset([string]$cache) {
    Remove-Item Function:\Invoke-Starship-PreCommand -ea 0
    $global:_starshipDeferred = $true   # skip the first-prompt branch (it starts git fetch/pull and dotcli sync)
    $global:_starshipCache = $cache
    $global:j = $null; $global:syncJob = $null
    $global:hookCalls = 0; $global:zoxCalls = 0; $global:seen = @()
    Invoke-Expression $hookAst.Extent.Text
    $inner = $global:_promptHook
    $global:_promptHook = [scriptblock]::Create("`$global:hookCalls++; " + $inner.ToString())
    Invoke-Expression $promptText
}
function global:__zoxide_hook { $global:zoxCalls++ }
# The hook also drives the dotcli background processes; no jobs are started here, so no-op stubs suffice
function global:__dotcli_kickSync {}
function global:__dotcli_end($job) {}

# --- A: fake starship cache records $? and $LASTEXITCODE like the real one ---
$fake = Join-Path $tmp 'starship-init.ps1'
Set-Content $fake @'
function global:prompt {
    $origDollarQuestion = $global:?
    $origLastExitCode = $global:LASTEXITCODE
    try { if (Test-Path function:Invoke-Starship-PreCommand) { Invoke-Starship-PreCommand } } catch {}
    $global:seen += ,@($origDollarQuestion, $origLastExitCode)
    "starship> "
}
'@
Reset $fake
$e0 = $error.Count
Get-Item C:\nope -ea 0; $null = prompt          # 2nd prompt: deferred load path
Check 'A1 deferred load sees $?=False' ($global:seen[-1][0] -eq $false)
Check 'A1 error record not duplicated' ($error.Count -eq $e0 + 1)
Check 'A1 hook called once' ($global:hookCalls -eq 1)
Check 'A1 zoxide hook called' ($global:zoxCalls -eq 1)
Get-Item C:\nope -ea 0; $null = prompt          # later prompt: starship directly
Check 'A2 later prompt sees $?=False' ($global:seen[-1][0] -eq $false)
Check 'A2 hook called once per prompt' ($global:hookCalls -eq 2)
cmd.exe /c exit 3; $null = prompt
Check 'A3 later prompt sees exit 3' ($global:seen[-1][0] -eq $false -and $global:seen[-1][1] -eq 3)
$null = 1; $null = prompt
Check 'A4 success resets $?=True' ($global:seen[-1][0] -eq $true)
Reset $fake
cmd.exe /c exit 5; $null = prompt
Check 'A5 deferred load keeps exit 5' ($global:seen[-1][0] -eq $false -and $global:seen[-1][1] -eq 5)
Reset $fake
$null = 1; $null = prompt
Check 'A6 deferred load success $?=True' ($global:seen[-1][0] -eq $true)

# --- B: missing / empty cache must not recurse ---
foreach ($case in @(@('missing', (Join-Path $tmp 'nope.ps1')), @('empty', (Join-Path $tmp 'empty.ps1')))) {
    if ($case[0] -eq 'empty') { $null = New-Item -ItemType File $case[1] }
    Reset $case[1]
    $outs = 1..5 | ForEach-Object { prompt }
    Check "B $($case[0]) cache: 5 prompts return '<PWD>> '" (@($outs | Where-Object { $_ -eq "$PWD> " }).Count -eq 5)
    Check "B $($case[0]) cache: hook once per prompt" ($global:hookCalls -eq 5)
}

Remove-Item -Recurse -Force $tmp
if ($fail) { "$fail failed"; exit 1 } else { 'all passed' }
