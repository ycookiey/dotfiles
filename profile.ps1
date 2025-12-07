# Git Worktree Runner
$gtrBin = 'C:\Main\Script\git-worktree-runner\bin'
$pathEntries = $env:Path -split ';'
if (-not ($pathEntries -contains $gtrBin)) {
    $env:Path = ($pathEntries + $gtrBin) -join ';'
}

$gitGtrScript = Join-Path $gtrBin 'git-gtr.ps1'
if (Test-Path $gitGtrScript) {
    function git-gtr {
        & $gitGtrScript @args
    }

    Set-Alias -Name gtr -Value git-gtr -Scope Global
}

# BOMなしUTF-8のインスタンスを作成
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

# 入出力のエンコーディングをUTF-8（BOMなし）に
[Console]::OutputEncoding = $utf8NoBom
[Console]::InputEncoding = $utf8NoBom
$OutputEncoding = $utf8NoBom
$PSDefaultParameterValues['Out-File:Encoding'] = 'utf8NoBOM'

# Antigravity alias
function agy {
    antigravity .
}

# zoxide
Invoke-Expression (& { (zoxide init powershell | Out-String) })