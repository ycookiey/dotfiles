# ==========================================
# 1. Config & Auto Update (Async)
# ==========================================
$DotfilesDir = 'C:\Main\Project\dotfiles'

# PowerShell の `git` を関数で上書きして、常に Scoop の git.exe を使う（PATH 上の別 git.exe を無視）
# ただし `git summary` 等の外部サブコマンドは、起動した git.exe が自身の探索パス（exec-path / PATH）から見つけて実行する
function git { & "$env:USERPROFILE\scoop\shims\git.exe" @args }

function Start-DotfilesAutoUpdateJob {
    param([Parameter(Mandatory)][string]$RepoDir)
    Start-ThreadJob {
        Set-Location $using:RepoDir
        git fetch -q
        git diff --quiet HEAD '@{u}'
        if (-not $?) { git pull -q -r --autostash; $true }
    }
}

if ($global:j) { Remove-Job $global:j -Force -ErrorAction SilentlyContinue }

$global:j = Start-DotfilesAutoUpdateJob -RepoDir $DotfilesDir


# ==========================================
# 2. Environment & Encodings
# ==========================================
# BOMなしUTF-8設定
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8NoBom
[Console]::InputEncoding = $utf8NoBom
$OutputEncoding = $utf8NoBom
$PSDefaultParameterValues['Out-File:Encoding'] = 'utf8NoBOM'

# Starship 設定ファイルのパス
$env:STARSHIP_CONFIG = Join-Path $DotfilesDir 'starship.toml'


# ==========================================
# 3. Tools & Aliases
# ==========================================
function Add-PathEntryIfMissing {
    param([Parameter(Mandatory)][string]$PathEntry)
    $paths = $env:Path -split ';' | Where-Object { $_ }
    if ($paths -notcontains $PathEntry) {
        $env:Path = ($paths + $PathEntry) -join ';'
    }
}

# Git Worktree Runner
$gtrBin = 'C:\Main\Script\git-worktree-runner\bin'
Add-PathEntryIfMissing -PathEntry $gtrBin
$gitGtrScript = Join-Path $gtrBin 'git-gtr.ps1'
if (Test-Path $gitGtrScript) {
    function git-gtr { & $gitGtrScript @args }
    Set-Alias -Name gtr -Value git-gtr -Scope Global
}

# MiKTeX (LaTeX)
$miktexBin = Join-Path $env:USERPROFILE 'scoop\apps\miktex\current\texmfs\install\miktex\bin\x64'
Add-PathEntryIfMissing -PathEntry $miktexBin


function grf { gh repo list $args -L 1000 --json nameWithOwner,description,url -q '.[]|[.nameWithOwner,.description,.url]|@tsv' | fzf -d "`t" --with-nth 1,2 | %{$_.Split("`t")[-1]} }
function grfo { ii (grf) }
function grfc { gh repo clone (grf) }
function agy { antigravity . }
function lg { lazygit }
function c { if ($args[0] -eq 'r') { claude /resume @($args[1..999]) } else { claude @args } }
function z- { z - }

function Start-App($Name) {
    explorer "shell:AppsFolder\$((Get-StartApps $Name | select -f 1).AppID)"
}
function kindle { explorer kindle: }
function dis { explorer discord: }
function slk { explorer slack: }
function obsd { obsidian }
function cal { Start-App "Notion Calendar" }
function viv { vivaldi }

# ==========================================
# 4. Initialize Tools
# ==========================================
Invoke-Expression (& { (zoxide init powershell | Out-String) })
Invoke-Expression (&starship init powershell)

# zoxide自動学習用フック
function Invoke-Starship-PreCommand { $null = __zoxide_hook }


# ==========================================
# 5. Prompt Hook 
# ==========================================
$oldPrompt = $function:prompt
function prompt {
    if ($global:j -and $global:j.State -eq 'Completed') {
        if (Receive-Job $global:j) { Write-Host "`n✨ Dotfiles Updated!" -Fg Green }
        Remove-Job $global:j; $global:j = $null
    }
    & $oldPrompt
}