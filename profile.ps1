$DotfilesDir = 'C:\Main\Project\dotfiles'

$global:j = Start-ThreadJob {
    Set-Location $using:DotfilesDir
    git fetch -q
    git diff --quiet HEAD '@{u}'
    if (-not $?) { git pull -q -r --autostash; $true }
}

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

# Launch Electron app with suppressed output
function Start-ElectronApp {
    param(
        [Parameter(Mandatory=$true)]
        [string]$ExePath,

        [Parameter(Mandatory=$false)]
        [string[]]$Arguments = @()
    )

    $dummyLog = Join-Path $env:TEMP "$([System.IO.Path]::GetFileNameWithoutExtension($ExePath))_output.tmp"

    Start-Process -FilePath $ExePath `
        -ArgumentList $Arguments `
        -RedirectStandardError "NUL" `
        -RedirectStandardOutput $dummyLog `
        -WindowStyle Normal
}

# Fuzzy-search and open GitHub repositories (ghf [user])
function ghf {
    gh repo list $args --limit 1000 --json url --jq '.[].url' | fzf | %{ start $_ }
}

# Launch Vivaldi browser (viv [url])
function viv {
    & "$env:LOCALAPPDATA\Vivaldi\Application\vivaldi.exe" @args
}

# Launch Obsidian
function obsd {
    & "$env:LOCALAPPDATA\Programs\Obsidian\Obsidian.exe" @args
}

# Launch Slack
function slk {
    & "$env:LOCALAPPDATA\Microsoft\WindowsApps\Slack.exe" @args
}

# Launch Notion Calendar
function cal {
    Start-ElectronApp -ExePath "$env:LOCALAPPDATA\Programs\notion-calendar-web\Notion Calendar.exe" -Arguments $args
}

# Launch Discord
function dis {
    & "$env:LOCALAPPDATA\Discord\Update.exe" --processStart Discord.exe
}

# zoxide
Invoke-Expression (& { (zoxide init powershell | Out-String) })


$oldPrompt = $function:prompt
function prompt {
    if ($global:j.State -eq 'Completed') {
        if (Receive-Job $global:j) { Write-Host "`n✨ Dotfiles Updated!" -Fg Green }
        Remove-Job $global:j; $global:j = $null
    }
    & $oldPrompt
}