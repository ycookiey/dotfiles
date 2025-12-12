# ==========================================
# 1. Config & Auto Update (Async)
# ==========================================
$DotfilesDir = 'C:\Main\Project\dotfiles'

if ($global:j) { Remove-Job $global:j -Force -ErrorAction SilentlyContinue }

$global:j = Start-ThreadJob {
    Set-Location $using:DotfilesDir
    git fetch -q
    git diff --quiet HEAD '@{u}'
    if (-not $?) { git pull -q -r --autostash; $true }
}


# ==========================================
# 2. Environment & Encodings
# ==========================================
# BOMなしUTF-8設定 
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8NoBom
[Console]::InputEncoding = $utf8NoBom
$OutputEncoding = $utf8NoBom
$PSDefaultParameterValues['Out-File:Encoding'] = 'utf8NoBOM'


# ==========================================
# 3. Tools & Aliases
# ==========================================
# Git Worktree Runner
$gtrBin = 'C:\Main\Script\git-worktree-runner\bin'
if (-not ($env:Path -split ';' -contains $gtrBin)) {
    $env:Path = ($env:Path -split ';' | Where-Object { $_ } ) + $gtrBin -join ';'
}
$gitGtrScript = Join-Path $gtrBin 'git-gtr.ps1'
if (Test-Path $gitGtrScript) {
    function git-gtr { & $gitGtrScript @args }
    Set-Alias -Name gtr -Value git-gtr -Scope Global
}


function Start-ElectronApp {
    param([string]$ExePath, [string[]]$Arguments = @())
    $dummyLog = Join-Path $env:TEMP "$([IO.Path]::GetFileNameWithoutExtension($ExePath))_output.tmp"
    Start-Process -FilePath $ExePath -ArgumentList $Arguments -RedirectStandardError "NUL" -RedirectStandardOutput $dummyLog -WindowStyle Normal
}
function ghf { 
    gh repo list $args --limit 1000 --json url --jq '.[].url' | fzf | %{ start $_ } 
}

function agy { antigravity . }
function viv { & "$env:LOCALAPPDATA\Vivaldi\Application\vivaldi.exe" @args }
function obsd { & "$env:LOCALAPPDATA\Programs\Obsidian\Obsidian.exe" @args }
function slk { & "$env:LOCALAPPDATA\Microsoft\WindowsApps\Slack.exe" @args }
function dis { & "$env:LOCALAPPDATA\Discord\Update.exe" --processStart Discord.exe }
function cal { Start-ElectronApp -ExePath "$env:LOCALAPPDATA\Programs\notion-calendar-web\Notion Calendar.exe" -Arguments $args }

# ==========================================
# 4. Initialize Tools
# ==========================================
Invoke-Expression (& { (zoxide init powershell | Out-String) })


# ==========================================
# 5. Prompt Hook 
# ==========================================
$oldPrompt = $function:prompt
function prompt {
    if ($global:j.State -eq 'Completed') {
        if (Receive-Job $global:j) { Write-Host "`n✨ Dotfiles Updated!" -Fg Green }
        Remove-Job $global:j; $global:j = $null
    }
    & $oldPrompt
}