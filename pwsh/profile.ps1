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

$chromeYatcyScript = "$Dot\bin\chrome-yatcy.ps1"
if ([IO.File]::Exists($chromeYatcyScript)) {
    function chrome-yatcy { & $chromeYatcyScript @args }
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
    # dotcli 裏処理の throttle 間隔: 前回起動 (stamp file の mtime) から間隔内なら起動しない
    $global:_gitPromptInterval = [timespan]'0:10:0'  # git fetch / pull -r (dotfiles 自動更新)
    $global:_syncInterval = [timespan]'1.0:0:0'      # settings / scoop・winget export / MCP sync

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
    $starshipCache = $global:_starshipCache = "$cacheDir\starship-init.ps1"
    if (![IO.File]::Exists($starshipCache) -or
        [IO.File]::GetLastWriteTime($starshipExe) -gt [IO.File]::GetLastWriteTime($starshipCache)) {
        & $starshipExe init powershell --print-full-init > $starshipCache
    }

    # --- Dotfiles background jobs (dotcli git-prompt → sync, build --check) ---
    # dotcli を絶対 path で別 process として起動し、prompt hook が完了を見て結果を表示する。
    # $global:j = git-prompt (自動更新), $global:syncJob = sync, $global:_buildCheckJob = build --check
    $global:_dotDir = $Dot
    $global:_dotcli = (Get-Command dotcli -CommandType Application -ea 0 | select -First 1).Source
    $global:_gitPromptStamp = "$cacheDir\git-prompt.stamp"
    $global:_syncStamp = "$cacheDir\sync.stamp"
    # stamp が無いと GetLastWriteTimeUtc は 1601 年を返すので常に due
    function global:__dotcli_due($stamp, $interval) {
        [DateTime]::UtcNow - [IO.File]::GetLastWriteTimeUtc($stamp) -ge $interval
    }
    # 起動に成功したら $stamp を touch し @{Process; Out; Err} を返す。失敗時は 1 行表示して $null
    function global:__dotcli_start([string[]]$a, [string]$stamp) {
        try {
            $si = [Diagnostics.ProcessStartInfo]::new($global:_dotcli)
            foreach ($x in $a) { $si.ArgumentList.Add($x) }
            $si.UseShellExecute = $false
            $si.CreateNoWindow = $true
            $si.RedirectStandardInput = $si.RedirectStandardOutput = $si.RedirectStandardError = $true
            $si.StandardOutputEncoding = $si.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
            $si.WorkingDirectory = $global:_dotDir  # 利用者の cwd を子 process が握り続けないように
            $pr = [Diagnostics.Process]::Start($si)
            $pr.StandardInput.Close()  # 子 (git / scoop / winget) が入力待ちで止まらないよう即 EOF
            # stdout/stderr は両方非同期で読む (片方を同期で読むと他方の pipe が詰まり deadlock)
            $job = @{ Process = $pr; Out = $pr.StandardOutput.ReadToEndAsync(); Err = $pr.StandardError.ReadToEndAsync() }
            if ($stamp) { try { [IO.File]::WriteAllBytes($stamp, [byte[]]@()) } catch {} }
            $job
        } catch {
            wh "`n✗ dotcli $($a[0]): failed to start ($($_.Exception.Message))" -Fo Red
        }
    }
    # 完了済みなら Process を Dispose して @{Out; Err} を返す。未完了なら $null
    function global:__dotcli_end($job) {
        if (!$job.Out.IsCompleted -or !$job.Err.IsCompleted) { return }
        try { @{ Out = $job.Out.Result; Err = $job.Err.Result } }
        catch { @{ Out = ''; Err = '' } }
        finally { $job.Process.Dispose() }
    }
    # sync は git-prompt の pull と同じ tracked file (install/scoopfile.json 等) を書き換えうるので、
    # git-prompt の完了後 ($global:j が空) に起動して pull と直列化する
    function global:__dotcli_kickSync {
        if (!$global:_syncPending -or $global:j) { return }
        $global:_syncPending = $false
        if (__dotcli_due $global:_syncStamp $global:_syncInterval) {
            $global:syncJob = __dotcli_start @('sync', '--dot', $global:_dotDir) $global:_syncStamp
        }
    }

    # --- Prompt hook (inlined) ---
    $global:_promptHook = {
        if ($global:j -and ($r = __dotcli_end $global:j)) {
            $global:j = $null
            $v = $r.Out.Trim()
            if ($v.StartsWith('updated:')) {
                $p = $v.Substring(8) -split ':', 2
                $label = $p[0] -eq '1' ? 'commit' : 'commits'
                wh "`n✨ Dotfiles Updated! ($($p[0]) ${label}: $($p[1]))" -Fo Green
            } elseif ($v -eq 'dirty') {
                wh "`n⚠ Dotfiles: dirty (behind remote)" -Fo Yellow
            } elseif ($v -eq 'error') {
                wh "`n✗ Dotfiles: pull failed" -Fo Red
            }
        }
        __dotcli_kickSync
        if ($global:syncJob -and ($r = __dotcli_end $global:syncJob)) {
            $global:syncJob = $null
            # stderr (scoop / winget 未導入等) は従来どおり表示しない
            if ($r.Out.Trim() -eq 'synced') { wh "`n🔄 Synced" -Fo Cyan }
        }
        if ($global:_buildCheckJob -and ($r = __dotcli_end $global:_buildCheckJob)) {
            $global:_buildCheckJob = $null
            $t = ($r.Out + $r.Err).TrimEnd()  # "dotfiles build outdated" ... は stderr に出る
            if ($t) { wh "`n$t" }
        }
        $p = $PWD.Path -replace '\\','/'
        if (!$global:_hostName) { $global:_hostName = [Net.Dns]::GetHostName() }
        [Console]::Write("`e]7;file://$($global:_hostName)/$p`e\")
    }
    # 初回: 簡易プロンプト + dotfiles更新開始 → 2回目: starship 遅延読み込み → 以降: starship
    # starship の prompt は $? / $LASTEXITCODE を退避してから Invoke-Starship-PreCommand を呼ぶ。
    # hook を prompt の外側で先に実行すると $? が True に上書きされ error 表示が消えるので、hook は PreCommand 内で呼ぶ
    function global:prompt {
        # 必ず最初の文: これより前の文は $? を上書きする
        $q = $?; $c = $global:LASTEXITCODE
        if (!$global:_starshipDeferred) {
            & $global:_promptHook
            $global:_starshipDeferred = $true
            # Dotfiles 裏処理 (起動のみ。結果は 2 回目以降の prompt で hook が表示)
            if ($global:_dotcli) {
                if (__dotcli_due $global:_gitPromptStamp $global:_gitPromptInterval) {
                    $global:j = __dotcli_start @('git-prompt', $global:_dotDir) $global:_gitPromptStamp
                }
                $global:_syncPending = $true
                __dotcli_kickSync  # git-prompt を起動しなかった (間隔内 / 起動失敗) ときは即 sync
                $global:_buildCheckJob = __dotcli_start @('build', '--check')
            } else {
                wh "⚠ dotcli not found (dotfiles auto-update / sync disabled): run $($global:_dotDir)\setup.ps1" -Fo Yellow
            }
            return "$PWD> "
        }
        # cache が無い/空 (starship 未導入等) のまま dot-source すると prompt が再定義されず、
        # 自分自身を呼び続けて hang するので、starship が prompt を再定義したことまで確かめる
        $self = $function:prompt.ToString()
        $cache = [IO.FileInfo]$global:_starshipCache
        if ($cache.Exists -and $cache.Length -gt 0) { . $cache.FullName }
        if ($function:prompt.ToString() -ne $self) {
            function global:Invoke-Starship-PreCommand {
                if (Test-Path Function:\_mise_hook) { _mise_hook }
                $null = __zoxide_hook
                & $global:_promptHook
            }
            # hook は starship の prompt が PreCommand 経由で呼ぶので、ここでは呼ばない (二重通知防止)
            $sp = $function:prompt
            $global:LASTEXITCODE = $c
            if (-not $q) { Write-Error '' -ea Ignore }  # $? を False に戻す ($error には残さない)
            & $sp
            return
        }
        # starship を使えない: 簡易プロンプトに恒久切り替え
        function global:prompt {
            if (Test-Path Function:\_mise_hook) { _mise_hook }
            $null = __zoxide_hook
            & $global:_promptHook
            "$PWD> "
        }
        & $function:prompt
    }
}
