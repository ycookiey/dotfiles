# ==========================================
# 0. Interactive Detection
# ==========================================
$script:IsInteractive = !(
    [Environment]::GetCommandLineArgs() | ? { $_ -eq '-NonInteractive' }
)

# ==========================================
# 1. Config & Auto Update (Async)
# ==========================================
$DotfilesDir = 'C:\Main\Project\dotfiles'

# Scoop の git.exe を強制使用
function git { & "$HOME\scoop\shims\git.exe" @args }

if ($script:IsInteractive) {
    function Start-DotfilesAutoUpdateJob {
        param([Parameter(Mandatory)][string]$RepoDir)
        Start-ThreadJob {
            cd $using:RepoDir
            git fetch -q
            git diff --quiet HEAD '@{u}'
            if (!$?) { git pull -q -r --autostash; $true }
        }
    }

    if ($global:j) { Remove-Job $global:j -Force -ea 0 }

    $global:j = Start-DotfilesAutoUpdateJob -RepoDir $DotfilesDir
}

# ==========================================
# 2. Environment & Encodings
# ==========================================
$utf8NoBom = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8NoBom
[Console]::InputEncoding = $utf8NoBom
$OutputEncoding = $utf8NoBom
$PSDefaultParameterValues['Out-File:Encoding'] = 'utf8NoBOM'

$env:STARSHIP_CONFIG = "$DotfilesDir\starship.toml"
$env:YAZI_FILE_ONE = "$HOME\scoop\apps\git\current\usr\bin\file.exe"

# ==========================================
# 3. Tools & Aliases
# ==========================================
. "$DotfilesDir\aliases.ps1"
function Add-PathEntryIfMissing {
    param([Parameter(Mandatory)][string]$PathEntry)
    $paths = $env:Path -split ';' | ? { $_ }
    if ($PathEntry -notin $paths) {
        $env:Path = ($paths + $PathEntry) -join ';'
    }
}

$gtrBin = 'C:\Main\Script\git-worktree-runner\bin'
Add-PathEntryIfMissing $gtrBin
$gitGtrScript = "$gtrBin\git-gtr.ps1"
if (tp $gitGtrScript) {
    function git-gtr { & $gitGtrScript @args }
    sal -Name gtr -Value git-gtr -Scope Global
}

$miktexBin = "$HOME\scoop\apps\miktex\current\texmfs\install\miktex\bin\x64"
Add-PathEntryIfMissing $miktexBin

$androidPlatformTools = "$env:LOCALAPPDATA\Android\Sdk\platform-tools"
Add-PathEntryIfMissing $androidPlatformTools

function gnew { & "$DotfilesDir\bin\git-new.ps1" @args }
function toggle-theme { & "$DotfilesDir\bin\toggle-theme.ps1" @args }
function admin { start wezterm -Verb RunAs -Arg 'start','--cwd',$PWD }

function grf { gh repo list $args -L 1000 --json nameWithOwner,description,url -q '.[]|[.nameWithOwner,.description,.url]|@tsv' | fzf -d "`t" --with-nth 1,2 | %{$_.Split("`t")[-1]} }
function grfo { start (grf) }
function grfc { gh repo clone (grf) }
function locked($Path='.') {sudo handle (Resolve-Path $Path).Path.TrimEnd('\')}
function agy { antigravity . }
function lg { lazygit }
function frun {
    $adb = "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe"
    $flavors = @('develop', 'staging', 'production')
    $emu = flutter emulators 2>$null | sls '•' | % { ($_ -split '•')[0].Trim() } | select -Skip 1 | fzf --prompt='Emulator: '
    if (!$emu) { return }
    $flavor = $flavors | fzf --prompt='Flavor: '
    if (!$flavor) { return }
    $before = (& $adb devices | sls 'emulator').Count
    flutter emulators --launch $emu
    if ((& $adb devices | sls 'emulator').Count -gt $before) {
        & $adb wait-for-device
        & $adb shell 'while [[ -z $(getprop sys.boot_completed) ]]; do sleep 1; done'
    }
    flutter run --flavor $flavor
}
function v { nvim @args }
function c {
    $d = "$HOME/.claude"; $a = @($args); $lf = "$d/.last_account"
    if ($a[0] -eq 'save') { cp "$d/.credentials.json" "$d-$($a[1])/.credentials.json"; echo "Account $($a[1]) saved"; return }
    $n = if ($a[0] -match '^\d+$') { $a[0]; $a = $a[1..99] } elseif (tp $lf) { gc $lf -Raw }
    if ($n) {
        if (!(tp "$d-$n")) { echo "Not found. Run setup.ps1"; return }
        $env:CLAUDE_CONFIG_DIR = "$d-$n"; $n | sc $lf -No
    } else { rm env:CLAUDE_CONFIG_DIR -ea Ignore }
    if ($a[0] -eq 'r') { $a[0] = '/resume' }
    claude @a
}
function cb {
    $env:CLAUDE_CODE_USE_BEDROCK = "1"
    $env:AWS_REGION = "ap-northeast-1"
    $env:CLAUDE_CODE_MAX_OUTPUT_TOKENS = "4096"
    $env:ANTHROPIC_MODEL = "global.anthropic.claude-opus-4-5-20251101-v1:0"
    if ($args[0] -eq 'r') { claude /resume @($args[1..999]) } else { claude @args }
}
function z- { z - }
function f { fzf @args }
function fm { fzf -m @args }
function zp { zoxide query -i @args }
function y {
    $tmp = [IO.Path]::GetTempFileName()
    yazi $args --cwd-file=$tmp
    $cwd = gc $tmp
    if ($cwd -and $cwd -ne $PWD.Path) { cd $cwd }
    rm $tmp
}


if ($script:IsInteractive) {
    # ==========================================
    # 4. Initialize Tools
    # ==========================================
    iex (& { (zoxide init powershell | Out-String) })
    iex (&starship init powershell)

    # zoxide自動学習用フック
    function Invoke-Starship-PreCommand { $null = __zoxide_hook }

    # ==========================================
    # 5. Prompt Hook
    # ==========================================
    $oldPrompt = $function:prompt
    function prompt {
        if ($global:j -and $global:j.State -eq 'Completed') {
            if (Receive-Job $global:j) { wh "`n✨ Dotfiles Updated!" -Fg Green }
            Remove-Job $global:j; $global:j = $null
        }
        & $oldPrompt
    }
}
