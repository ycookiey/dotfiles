# ==========================================
# 0. Interactive Detection
# ==========================================
$script:IsInteractive = [Environment]::GetCommandLineArgs() -notcontains '-NonInteractive'

# ==========================================
# 1. Config & Environment
# ==========================================
$_me = Get-Item $PSCommandPath
$Dot = Split-Path $(if ($_me.Target) { Split-Path $_me.Target } else { $PSScriptRoot })
$Proj = Split-Path $Dot
$PSDefaultParameterValues['Out-File:Encoding'] = 'utf8NoBOM'
$env:STARSHIP_CONFIG = "$Dot\starship.toml"
$env:YAZI_FILE_ONE = "$HOME\scoop\apps\git\current\usr\bin\file.exe"

# ==========================================
# 2. Tools & Aliases
# ==========================================
. "$Dot\pwsh\aliases.ps1"
if ([IO.File]::Exists("$Dot\generated-aliases.ps1")) { . "$Dot\generated-aliases.ps1" }
$gtrBin = "$Proj\git-worktree-runner\bin"
$paths = $env:Path.Split(';', [StringSplitOptions]::RemoveEmptyEntries)
foreach ($p in @(
    $gtrBin
    "$HOME\scoop\apps\miktex\current\texmfs\install\miktex\bin\x64"
    "$env:LOCALAPPDATA\Android\Sdk\platform-tools"
)) { if ($p -notin $paths) { $paths += $p } }
$env:Path = $paths -join ';'

$gitGtrScript = "$Dot\bin\git-gtr.ps1"
if ([IO.File]::Exists($gitGtrScript)) {
    function git-gtr { & $gitGtrScript @args }
    $alias:gtr = 'git-gtr'
}

function admin { start wezterm -Verb RunAs -Arg 'start','--cwd',$PWD }
function vf { v (f @args) }
function y {
    $tmp = [IO.Path]::GetTempFileName()
    yazi @args --cwd-file=$tmp
    $d = (gc $tmp -ea 0 | select -First 1)?.Trim()
    if ($d -and $d -ne $PWD.Path) { cd $d }
    rm $tmp -ea 0
}


if ($script:IsInteractive) {
    # --- Proxy auto-detect (同期、PIDファイル + プロセス存在チェック) ---
    $_pidFile = 'C:\Main\Project\en-hancer-proxy\.en-hancer-proxy.pid'
    if ([IO.File]::Exists($_pidFile)) {
        try {
            [void][Diagnostics.Process]::GetProcessById([int][IO.File]::ReadAllText($_pidFile).Trim())
            $env:HTTP_PROXY = $env:HTTPS_PROXY = $env:ALL_PROXY = 'http://127.0.0.1:18080'
            $env:NO_PROXY = 'localhost,127.0.0.1'
        } catch {}
    }

    # ==========================================
    # 3. Initialize Tools
    # ==========================================

    # --- mise ---
    if (gcm mise -ea 0) { (mise activate pwsh) -join "`n" | iex }

    # --- zoxide (手書き最小版、dot-source不要) ---
    function global:__zoxide_pwd { $l = gl; if ($l.Provider.Name -eq 'FileSystem') { $l.ProviderPath } }
    function global:__zoxide_cd($dir, $literal) {
        if ($literal) { cd -LiteralPath $dir -PassThru -ea Stop }
        else { cd -Path $dir -PassThru -ea Stop }
    }
    function global:__zoxide_z {
        if (!$args.Length) { __zoxide_cd ~ $true }
        elseif ($args.Length -eq 1 -and ($args[0] -eq '-' -or $args[0] -eq '+')) { __zoxide_cd $args[0] $false }
        elseif ($args.Length -eq 1 -and (tp -PathType Container -LiteralPath $args[0])) { __zoxide_cd $args[0] $true }
        elseif ($args.Length -eq 1 -and (tp -PathType Container -Path $args[0])) { __zoxide_cd $args[0] $false }
        else {
            $r = __zoxide_pwd
            $result = if ($null -ne $r) { zoxide query --exclude $r "--" @args } else { zoxide query "--" @args }
            if ($LASTEXITCODE -eq 0) { __zoxide_cd $result $true }
        }
    }
    function global:__zoxide_zi {
        $result = zoxide query -i "--" @args
        if ($LASTEXITCODE -eq 0) { __zoxide_cd $result $true }
    }
    $alias:z = '__zoxide_z'
    $alias:zi = '__zoxide_zi'
    $global:__zoxide_oldpwd = $PWD.ProviderPath
    function global:__zoxide_hook {
        $result = __zoxide_pwd
        if ($result -ne $global:__zoxide_oldpwd) {
            if ($null -ne $result) { zoxide add "--" $result }
            $global:__zoxide_oldpwd = $result
        }
    }

    # --- starship (cached) ---
    $cacheDir = "$HOME\.cache\pwsh"
    if (![IO.Directory]::Exists($cacheDir)) { mkd $cacheDir }
    $starshipExe = "$HOME\scoop\apps\starship\current\starship.exe"
    $starshipCache = "$cacheDir\starship-init.ps1"
    if (![IO.File]::Exists($starshipCache) -or
        [IO.File]::GetLastWriteTime($starshipExe) -gt [IO.File]::GetLastWriteTime($starshipCache)) {
        & $starshipExe init powershell --print-full-init > $starshipCache
    }

    # --- Prompt hook (inlined) ---
    $global:_promptHook = {
        if ($global:j -and $global:j.Handle.IsCompleted) {
            try {
                $r = $global:j.PowerShell.EndInvoke($global:j.Handle)
                if ($r -and $r.Count -gt 0) { wh "`n✨ Dotfiles Updated!" -ForegroundColor Green }
            } finally { $global:j.PowerShell.Dispose(); $global:j = $null }
        }
        if ($global:syncJob -and $global:syncJob.Handle.IsCompleted) {
            try {
                $r = $global:syncJob.PowerShell.EndInvoke($global:syncJob.Handle)
                if ($r -and $r.Count -gt 0) { wh "`n🔄 Synced" -ForegroundColor Cyan }
            } finally { $global:syncJob.PowerShell.Dispose(); $global:syncJob = $null }
        }
        $p = $PWD.Path -replace '\\','/'
        if (!$global:_hostName) { $global:_hostName = [Net.Dns]::GetHostName() }
        [Console]::Write("`e]7;file://$($global:_hostName)/$p`e\")
    }
    # 初回: 簡易プロンプト + dotfiles更新開始 → 2回目: starship 遅延読み込み → 以降: starship
    function global:prompt {
        & $global:_promptHook
        if (!$global:_starshipDeferred) {
            $global:_starshipDeferred = $true
            # Dotfiles auto-update (初回プロンプトで遅延起動)
            $_ps = [PowerShell]::Create()
            [void]$_ps.AddScript(@"
                Set-Location '$Dot'
                & '$HOME\scoop\apps\git\current\cmd\git.exe' status --porcelain | ForEach-Object { return }
                & '$HOME\scoop\apps\git\current\cmd\git.exe' fetch -q
                & '$HOME\scoop\apps\git\current\cmd\git.exe' diff --quiet HEAD '@{u}'
                if (-not `$?) { & '$HOME\scoop\apps\git\current\cmd\git.exe' pull -q -r; `$true }
"@)
            $global:j = @{ PowerShell = $_ps; Handle = $_ps.BeginInvoke() }
            # Sync (MCP servers etc.)
            $_syncPs = [PowerShell]::Create()
            [void]$_syncPs.AddScript("& '$Dot\pwsh\sync.ps1' -Dot '$Dot'")
            $global:syncJob = @{ PowerShell = $_syncPs; Handle = $_syncPs.BeginInvoke() }
            return "$PWD> "
        }
        . "$HOME\.cache\pwsh\starship-init.ps1"
        function global:Invoke-Starship-PreCommand { if (Test-Path Function:\_mise_hook) { _mise_hook }; $null = __zoxide_hook }
        $global:_starshipFn = $function:prompt
        function global:prompt {
            & $global:_promptHook
            & $global:_starshipFn
        }
        & $global:_starshipFn
    }
}
