# test-install.ps1 — install/ と setup.ps1 の静的検査。ファイルを読むだけで、何もインストール・変更しない
# 実行: pwsh -NoProfile -File tests/test-install.ps1   (FAIL が 1 件でもあれば exit 1)
#
# (A) リポジトリ内の全 .ps1 (.git, node_modules, .claude/worktrees を除く) が構文エラーなくパースできる
# (B) install/*.ps1 が pwsh/aliases.ps1 の関数・alias (wh, tp, sc, mkd, mkl, isadmin, Invoke-Skippable 等) を使うなら、
#     最初に使うより前に aliases.ps1 を dot-source している。
#     setup.ps1 から & で呼ばれたときは呼び出し元の定義が見えるので動くが、単体実行や別プロセス (pwsh -File) では
#     wh/tp は未定義になり、sc は sc.exe (サービス制御) に解決される
# (C) install/*.json がパースできる。install-order.json の large が scoopfile.json の apps にある。
#     scoopfile.json と wingetfile.json の中で ID が重複しない
# (D) setup.ps1 の mkl <link> "$ScriptDir\..." のリンク先がリポジトリに存在する
$ErrorActionPreference = 'Stop'
$Root = Split-Path $PSScriptRoot
$failed = 0

function Test-Pass($msg) { Write-Host "  PASS: $msg" -ForegroundColor Green }
function Test-Fail($msg) { Write-Host "  FAIL: $msg" -ForegroundColor Red; $script:failed++ }
function Test-Info($msg) { Write-Host "  INFO: $msg" -ForegroundColor Yellow }

function Read-Ast($path) {
    $tokens = $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
    [pscustomobject]@{ Ast = $ast; Errors = $errors }
}

function Get-RelPath($path) { [System.IO.Path]::GetRelativePath($Root, $path) }

# === (A) 構文検査 ===
Write-Host "`n[A] Parse every .ps1 in the repo" -ForegroundColor Cyan

# 対象は git が扱うファイル (tracked + ignore されていない untracked)。ignore された .history/ (エディタの履歴)・
# node_modules・.claude/worktrees 等は repo の一部ではないので見ない。symlink はたどらない (リポジトリの外に出ないように)
function Get-RepoScripts($dir) {
    git -C $dir ls-files -co --exclude-standard -- '*.ps1' |
        % { Get-Item -LiteralPath (Join-Path $dir $_) -Force -ea 0 } |
        ? { $_ -and !$_.LinkType }
}

$scripts = @(Get-RepoScripts $Root)
if (!$scripts) { Test-Fail "no .ps1 found under $Root" }
$parseFailed = 0
foreach ($s in $scripts) {
    $r = Read-Ast $s.FullName
    if ($r.Errors) {
        $parseFailed++
        $first = $r.Errors[0]
        Test-Fail "$(Get-RelPath $s.FullName):$($first.Extent.StartLineNumber): $($first.Message) ($($r.Errors.Count) error(s))"
    }
}
if ($scripts -and !$parseFailed) { Test-Pass "$($scripts.Count) script(s) parsed without errors" }

# === (B) aliases.ps1 の import 漏れ ===
Write-Host "`n[B] install/*.ps1 dot-source aliases.ps1 before using its names" -ForegroundColor Cyan

$aliasesPath = Join-Path $Root 'pwsh\aliases.ps1'
$aliasesAst = (Read-Ast $aliasesPath).Ast
$provided = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
# トップレベルの function 定義 (入れ子の function は外から見えないので含めない)
$aliasesAst.FindAll({ $args[0] -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $false) |
    % { [void]$provided.Add($_.Name) }
# $alias:name = ... の代入
$aliasesAst.FindAll({
    $a = $args[0]
    $a -is [System.Management.Automation.Language.AssignmentStatementAst] -and
    $a.Left -is [System.Management.Automation.Language.VariableExpressionAst] -and
    $a.Left.VariablePath.DriveName -eq 'alias'
}, $false) | % { [void]$provided.Add(($_.Left.VariablePath.UserPath -split ':', 2)[1]) }
# Set-Alias / New-Alias name ... (最初の位置引数を名前とみなす)
$aliasesAst.FindAll({
    $args[0] -is [System.Management.Automation.Language.CommandAst] -and
    $args[0].GetCommandName() -in 'Set-Alias', 'New-Alias', 'sal', 'nal'
}, $false) | % {
    $arg = $_.CommandElements | select -Skip 1 | ? { $_ -is [System.Management.Automation.Language.StringConstantExpressionAst] } | select -First 1
    if ($arg) { [void]$provided.Add($arg.Value) }
}
# 既存の cmdlet を上書きする関数 (Copy-Item 等) は import しなくても同名の cmdlet が動くので対象外
$shadowing = @($provided | ? { Get-Command $_ -CommandType Cmdlet -ErrorAction SilentlyContinue })
$shadowing | % { [void]$provided.Remove($_) }

if ($provided.Count -eq 0) {
    Test-Fail "no function or alias extracted from pwsh/aliases.ps1"
} else {
    Test-Info "aliases.ps1 provides: $(($provided | sort) -join ', ')$(if ($shadowing) { " (ignored, shadow cmdlets: $($shadowing -join ', '))" })"
}

foreach ($s in Get-ChildItem (Join-Path $Root 'install') -File -Filter *.ps1) {
    $ast = (Read-Ast $s.FullName).Ast
    $localDefs = @($ast.FindAll({ $args[0] -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true).Name)
    $uses = @($ast.FindAll({ $args[0] -is [System.Management.Automation.Language.CommandAst] }, $true) | ? {
        $n = $_.GetCommandName()  # & $exe などは $null
        $n -and $provided.Contains($n) -and $n -notin $localDefs
    })
    if (!$uses) { Test-Pass "$($s.Name) uses no aliases.ps1 names"; continue }

    $names = ($uses | % { $_.GetCommandName() } | sort -Unique) -join ', '
    $firstUse = $uses | sort { $_.Extent.StartOffset } | select -First 1
    $import = $ast.FindAll({
        $args[0] -is [System.Management.Automation.Language.CommandAst] -and
        $args[0].InvocationOperator -eq [System.Management.Automation.Language.TokenKind]::Dot -and
        $args[0].CommandElements[0].Extent.Text -match 'pwsh[\\/]aliases\.ps1'
    }, $true) | sort { $_.Extent.StartOffset } | select -First 1

    if (!$import) {
        Test-Fail "$($s.Name) uses ($names) without dot-sourcing pwsh\aliases.ps1"
    } elseif ($import.Extent.StartOffset -gt $firstUse.Extent.StartOffset) {
        Test-Fail "$($s.Name) uses '$($firstUse.GetCommandName())' at line $($firstUse.Extent.StartLineNumber) before dot-sourcing aliases.ps1 at line $($import.Extent.StartLineNumber)"
    } else {
        Test-Pass "$($s.Name) uses ($names) and dot-sources aliases.ps1 first"
    }
}

# === (C) manifest ===
Write-Host "`n[C] install/*.json manifests" -ForegroundColor Cyan

$json = @{}
foreach ($f in Get-ChildItem (Join-Path $Root 'install') -File -Filter *.json) {
    try {
        $json[$f.Name] = Get-Content -LiteralPath $f.FullName -Raw | ConvertFrom-Json
        Test-Pass "$($f.Name) parses"
    } catch {
        Test-Fail "$($f.Name) does not parse: $($_.Exception.Message)"
    }
}

function Test-NoDuplicate($file, $ids) {
    if (!$ids) { Test-Fail "$file has no IDs"; return }
    $dups = @($ids | group | ? Count -gt 1 | % Name)  # group は大文字小文字を区別しない (scoop/winget と同じ)
    if ($dups) { Test-Fail "$file has duplicate IDs: $($dups -join ', ')" }
    else { Test-Pass "$file has no duplicate IDs ($($ids.Count))" }
}

$scoop = $json['scoopfile.json']
$order = $json['install-order.json']
$winget = $json['wingetfile.json']
if ($scoop) { Test-NoDuplicate 'scoopfile.json' @($scoop.apps.Name) }
if ($winget) { Test-NoDuplicate 'wingetfile.json' @($winget.apps.Id) }
if ($scoop -and $order) {
    if (!$order.large) {
        Test-Fail "install-order.json has no large list"
    } else {
        $missing = @($order.large | ? { $_ -notin $scoop.apps.Name })
        if ($missing) { Test-Fail "install-order.json large not in scoopfile.json apps: $($missing -join ', ')" }
        else { Test-Pass "install-order.json large ($($order.large.Count)) are all in scoopfile.json" }
    }
}
foreach ($need in 'scoopfile.json', 'install-order.json', 'wingetfile.json') {
    if (!(Test-Path (Join-Path $Root "install\$need"))) { Test-Fail "install\$need is missing" }
}

# === (D) setup.ps1 のリンク先 ===
Write-Host "`n[D] setup.ps1 mkl targets under `$ScriptDir exist" -ForegroundColor Cyan

# mkl は Invoke-Step { } の中にあるので入れ子のスクリプトブロックも探す
$setupAst = (Read-Ast (Join-Path $Root 'setup.ps1')).Ast
$mkls = @($setupAst.FindAll({
    $args[0] -is [System.Management.Automation.Language.CommandAst] -and
    $args[0].GetCommandName() -eq 'mkl' -and $args[0].CommandElements.Count -ge 3
}, $true))
$checked = 0
foreach ($m in $mkls) {
    $target = $m.CommandElements[2]
    # "$ScriptDir\..." の形で、$ScriptDir 以外の変数・式を含まないものだけを見る ($_.FullName 等は実行時にしか決まらない)
    if ($target -isnot [System.Management.Automation.Language.ExpandableStringExpressionAst] -or
        $target.Value -notmatch '^\$(\{ScriptDir\}|ScriptDir\b)(.*)$' -or
        $target.NestedExpressions.Count -ne 1) { continue }
    $rel = $Matches[2].TrimStart('\', '/')
    $checked++
    $line = $m.Extent.StartLineNumber
    if (Test-Path -LiteralPath (Join-Path $Root $rel)) { Test-Pass "setup.ps1:$line $rel" }
    else { Test-Fail "setup.ps1:$line mkl target '$rel' does not exist in the repo" }
}
if ($checked -eq 0) { Test-Fail "no mkl `"`$ScriptDir\...`" call found in setup.ps1" }

# === 結果 ===
Write-Host ""
if ($failed -eq 0) {
    Write-Host "All tests passed!" -ForegroundColor Green
} else {
    Write-Host "$failed test(s) FAILED" -ForegroundColor Red
    exit 1
}
