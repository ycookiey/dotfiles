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


function grf { gh repo list $args -L 1000 --json nameWithOwner,description,url -q '.[]|[.nameWithOwner,.description,.url]|@tsv' | fzf -d "`t" --with-nth 1,2 | %{$_.Split("`t")[-1]} }
function grfo { ii (grf) }
function grfc { gh repo clone (grf) }
function agy { antigravity . }

function Start-App($Name) {
    explorer "shell:AppsFolder\$((Get-StartApps $Name | select -f 1).AppID)"
}
function dis { explorer discord: }
function slk { explorer slack: }
function obsd { explorer obsidian: }
function cal { Start-App "Notion Calendar" }
function viv { start vivaldi }

# ==========================================
# 4. Initialize Tools
# ==========================================
Invoke-Expression (& { (zoxide init powershell | Out-String) })


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