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


function Start-ElectronApp {
    param([string]$ExePath, [string[]]$Arguments = @())
    $dummyLog = Join-Path $env:TEMP "$([IO.Path]::GetFileNameWithoutExtension($ExePath))_output.tmp"
    Start-Process -FilePath $ExePath -ArgumentList $Arguments -RedirectStandardError "NUL" -RedirectStandardOutput $dummyLog -WindowStyle Normal
}

function Set-AppFunction {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$ExePath,
        [ValidateSet('Direct', 'Electron')][string]$Mode = 'Direct',
        [string[]]$FixedArguments = @()
    )

    $exe = $ExePath
    $fixed = $FixedArguments

    $impl = if ($Mode -eq 'Electron') {
        {
            param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args)
            Start-ElectronApp -ExePath $exe -Arguments ($fixed + $Args)
        }.GetNewClosure()
    }
    else {
        {
            param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args)
            & $exe @($fixed + $Args)
        }.GetNewClosure()
    }

    Set-Item -Path "function:global:$Name" -Value $impl
}

function grf { gh repo list $args -L 1000 --json nameWithOwner,description,url -q '.[]|[.nameWithOwner,.description,.url]|@tsv' | fzf -d "`t" --with-nth 1,2 | %{$_.Split("`t")[-1]} }
function grfo { ii (grf) }
function grfc { gh repo clone (grf) }
function agy { antigravity . }

function Join-LocalAppData {
    param([Parameter(Mandatory)][string]$ChildPath)
    Join-Path $env:LOCALAPPDATA $ChildPath
}

$appFunctions = @(
    @{
        Name = 'viv'
        ExePath = (Join-LocalAppData 'Vivaldi\Application\vivaldi.exe')
    },
    @{
        Name = 'obsd'
        ExePath = (Join-LocalAppData 'Programs\Obsidian\Obsidian.exe')
    },
    @{
        Name = 'slk'
        ExePath = (Join-LocalAppData 'Microsoft\WindowsApps\Slack.exe')
    },
    @{
        Name = 'dis'
        ExePath = (Join-LocalAppData 'Discord\Update.exe')
        FixedArguments = @('--processStart', 'Discord.exe')
    },
    @{
        Name = 'cal'
        ExePath = (Join-LocalAppData 'Programs\notion-calendar-web\Notion Calendar.exe')
        Mode = 'Electron'
    }
)

foreach ($app in $appFunctions) {
    Set-AppFunction @app
}

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