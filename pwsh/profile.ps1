# ==========================================
# 0. Interactive Detection
# ==========================================
# pwsh 本体の CLI 解析 (src/Microsoft.PowerShell.ConsoleHost/host/msh/CommandLineParameterParser.cs) と同じ規則で
# 起動引数を読み、prompt を出す起動かを判定する。非対話 = -NonInteractive (後の -Interactive で解除)、または
# -Command / -File / -EncodedCommand / -CommandWithArgs があり、それより前に -NoExit が無い。
# 前置は - / -- / / (en/em dash・horizontal bar も可)、名前は最短一意長以上の前方一致 (大小無視)。
# 表は本体の if-else 連鎖と同じ順 (先に当たったものが勝つ)。位置引数と未知の switch は本体も -File の script path とみなす。
# -Command / -File / -CommandWithArgs 以降は command・script 側の引数なので見ない (その -NoExit も無効)。
# pwsh が CLI switch を追加・変更したら表も更新すること
$script:IsInteractive = & {
    param([string[]]$a)
    # @(名前, 最短一致, 種別) 種別: x=help/version (即終了) / noe=-NoExit / noni=-NonInteractive / i=-Interactive /
    # v=値を 1 つ取る / ec=値を 1 つ取る command / r=以降すべてが command・script / f=値を取らない switch
    $sw = @(
        @('version', 'v', 'x'), @('help', 'h', 'x'), @('?', '?', 'x'), @('login', 'l', 'f'),
        @('noexit', 'noe', 'noe'), @('noprofile', 'nop', 'f'), @('nologo', 'nol', 'f'), @('noninteractive', 'noni', 'noni'),
        @('socketservermode', 'so', 'f'), @('v2socketservermode', 'v2so', 'f'), @('servermode', 's', 'f'),
        @('namedpipeservermode', 'nam', 'f'), @('sshservermode', 'sshs', 'f'), @('noprofileloadtime', 'noprofileloadtime', 'f'),
        @('interactive', 'i', 'i'), @('configurationfile', 'configurationfile', 'v'), @('configurationname', 'config', 'v'),
        @('custompipename', 'cus', 'v'), @('commandwithargs', 'commandwithargs', 'r'), @('cwa', 'cwa', 'r'),
        @('command', 'c', 'r'), @('windowstyle', 'w', 'v'), @('file', 'f', 'r'),
        @('outputformat', 'o', 'v'), @('of', 'o', 'v'), @('inputformat', 'inp', 'v'), @('if', 'if', 'v'),
        @('executionpolicy', 'ex', 'v'), @('ep', 'ep', 'v'), @('encodedcommand', 'e', 'ec'), @('ec', 'e', 'ec'),
        @('encodedarguments', 'encodeda', 'v'), @('ea', 'ea', 'v'), @('settingsfile', 'settings', 'v'),
        @('sta', 'sta', 'f'), @('mta', 'mta', 'f'), @('workingdirectory', 'wo', 'v'), @('wd', 'wd', 'v'),
        @('removeworkingdirectorytrailingcharacter', 'removeworkingdirectorytrailingcharacter', 'f'),
        @('token', 'to', 'v'), @('utctimestamp', 'utc', 'v')
    )
    $run = $noExit = $noni = $false
    # [0] は pwsh 自身の path
    for ($i = 1; $i -lt $a.Length; $i++) {
        $k = $a[$i].Trim()
        if (!$k) { continue }
        if ($k -notmatch '^(?:([-–—―])\1?|/)') { $run = $true; break }
        $k = $k.Substring($Matches[0].Length)
        $kind = 'r'
        foreach ($s in $sw) {
            if ($k.Length -ge $s[1].Length -and $s[0].StartsWith($k, [StringComparison]::OrdinalIgnoreCase)) { $kind = $s[2]; break }
        }
        if ($kind -eq 'noe') { $noExit = $true }
        elseif ($kind -eq 'noni') { $noni = $true }
        elseif ($kind -eq 'i') { $noni = $false }
        elseif ($kind -eq 'v') { $i++ }
        elseif ($kind -eq 'ec') { $run = $true; $i++ }
        elseif ($kind -in 'r', 'x') { $run = $true; break }
    }
    !$noni -and (!$run -or $noExit)
} ([Environment]::GetCommandLineArgs())

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

function admin {
    # Start-Process は ArgumentList を quote せず空白で連結するだけなので、空白を含む cwd は自分で quote する。
    # ProviderPath で PSDrive 名・provider prefix を除く。末尾の \ (drive root の C:\) は閉じ quote を
    # escape してしまう (C:" になる) ので \\ に重ねる (TrimEnd だと C: = drive の current dir になる)
    $d = $PWD.ProviderPath
    if ($d.EndsWith('\')) { $d += '\' }
    start wezterm -Verb RunAs -ArgumentList "start --cwd `"$d`""
}
function y {
    $tmp = [IO.Path]::GetTempFileName()
    yazi @args --cwd-file=$tmp
    $d = (gc $tmp -ea 0 | select -First 1)?.Trim()
    # -Path は wildcard 解釈で [ ] を含む dir へ移動できないので -LiteralPath
    if ($d -and $d -ne $PWD.Path) { Set-Location -LiteralPath $d }
    rm $tmp -ea 0
}


if ($script:IsInteractive) {
    # dotcli 裏処理の throttle 間隔: 前回起動 (stamp file の mtime) から間隔内なら起動しない
    $global:_gitPromptInterval = [timespan]'0:10:0'  # git fetch / pull -r (dotfiles 自動更新)
    $global:_syncInterval = [timespan]'1.0:0:0'      # settings / scoop・winget export / MCP sync
    # 別 pane の git-prompt が完了を記録しないまま (pane 終了 / idle) のとき、sync を待たせる上限
    $global:_gitPromptGrace = [timespan]'0:5:0'

    # --- Proxy auto-detect (同期、PID file + process 検証) ---
    # pid file は `dotcli proxy on` が書く (cargo run の child PID)。消すのは proxy off / status だけなので、
    # crash や OS 再起動の後は file が残り、無関係な process が同じ PID を再利用しうる。
    # そこで ProcessName が cargo、かつ pid file の書込みより前に起動した process のときだけ env を設定する。
    # proxy の起動方法が cargo run 以外に変わったら ProcessName の判定も更新すること。
    # TCP 接続での確認は起動時間が増えるので行わない
    $_pidFile = "$Proj\en-hancer-proxy\.en-hancer-proxy.pid"
    if ([IO.File]::Exists($_pidFile)) {
        $_proxyProc = $null
        try {
            $_proxyProc = [Diagnostics.Process]::GetProcessById([int][IO.File]::ReadAllText($_pidFile).Trim())
            # spawn → pid file 書込みの順なので StartTime <= mtime。時刻の粒度のずれ用に 5 秒の余裕を持たせる。
            # StartTime は権限不足で例外になりうるが、その場合も catch で未設定 (安全側) になる
            if ($_proxyProc.ProcessName -eq 'cargo' -and
                $_proxyProc.StartTime.ToUniversalTime() -le [IO.File]::GetLastWriteTimeUtc($_pidFile).AddSeconds(5)) {
                $env:HTTP_PROXY = $env:HTTPS_PROXY = $env:ALL_PROXY = 'http://127.0.0.1:18080'
                $env:NO_PROXY = 'localhost,127.0.0.1'
            }
        } catch {
        } finally {
            if ($_proxyProc) { $_proxyProc.Dispose() }
        }
    }

    # ==========================================
    # 3. Initialize Tools
    # ==========================================

    # --- mise ---
    # 種類を指定しない Get-Command は miss のとき module の command 探索まで走るので、Application に限る。
    # 候補が複数 (mise.exe と shim 等) でも 1 つに絞り、絶対 path で呼ぶ
    $_miseExe = (Get-Command mise -CommandType Application -ea 0 | select -First 1).Source
    if ($_miseExe) { (& $_miseExe activate pwsh) -join "`n" | iex }

    # --- zoxide (手書き最小版、dot-source不要) ---
    # bare の zoxide は scoop\shims\zoxide.exe (shim → exe の 2 process) になるので、実体を起動時に 1 回だけ解決する。
    # scoop 以外の install では PATH 上の zoxide に fallback
    $_zoxideApp = "$HOME\scoop\apps\zoxide\current\zoxide.exe"
    $global:_zoxideExe = [IO.File]::Exists($_zoxideApp) ? $_zoxideApp : 'zoxide'
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
            $result = if ($null -ne $r) { & $global:_zoxideExe query --exclude $r "--" @args } else { & $global:_zoxideExe query "--" @args }
            if ($LASTEXITCODE -eq 0) { __zoxide_cd $result $true }
        }
    }
    function global:__zoxide_zi {
        $result = & $global:_zoxideExe query -i "--" @args
        if ($LASTEXITCODE -eq 0) { __zoxide_cd $result $true }
    }
    $alias:z = '__zoxide_z'
    $alias:zi = '__zoxide_zi'
    $global:__zoxide_oldpwd = $PWD.ProviderPath
    function global:__zoxide_hook {
        $result = __zoxide_pwd
        if ($result -ne $global:__zoxide_oldpwd) {
            if ($null -ne $result) { & $global:_zoxideExe add "--" $result }
            $global:__zoxide_oldpwd = $result
        }
    }

    # --- starship (cached) ---
    $cacheDir = "$HOME\.cache\pwsh"
    if (![IO.Directory]::Exists($cacheDir)) { mkd $cacheDir }
    $starshipCur = "$HOME\scoop\apps\starship\current"
    $starshipExe = "$starshipCur\starship.exe"
    $starshipCache = $global:_starshipCache = "$cacheDir\starship-init.ps1"
    if ([IO.File]::Exists($starshipExe)) {
        # 1 行目の key で無効化を判定する。scoop は exe の mtime を archive のまま保持するので、
        # exe と cache の mtime 比較では古い build 日付の版への更新を検知できない。
        # key = current junction の解決先 (version dir) | exe の mtime | profile.ps1 の mtime (引数・後処理の変更用)
        # profile.ps1 は symlink 経由だと link 自身の mtime になるので、解決済みの $Dot 側を見る
        $_target = [IO.Directory]::ResolveLinkTarget($starshipCur, $false)?.FullName ?? $starshipCur
        $_key = "# key: $_target|$([IO.File]::GetLastWriteTimeUtc($starshipExe).Ticks)|$([IO.File]::GetLastWriteTimeUtc("$Dot\pwsh\profile.ps1").Ticks)"
        $_line1 = $null
        if ([IO.File]::Exists($starshipCache)) {
            try {
                $_sr = [IO.StreamReader]::new($starshipCache)
                try { $_line1 = $_sr.ReadLine() } finally { $_sr.Dispose() }
            } catch {}
        }
        if ($_line1 -ne $_key) {
            $_init = & $starshipExe init powershell --print-full-init
            if ($LASTEXITCODE -eq 0 -and $_init) {
                # starship は shim (scoop\shims\starship.exe) の path を埋め込み、prompt ごとに shim → exe の 2 process になる。
                # 実体を直接呼ぶよう置換する (出力形式が変わり当たらなくても shim 経由で動作は続く)
                $_lit = "'" + $starshipExe.Replace("'", "''") + "'"
                $_init = $_init -replace "Executable '[^']*starship\.exe'", "Executable $($_lit.Replace('$', '$$'))"
                # 並行起動した別 pwsh が書きかけを dot-source しないよう、一時 file から置き換える
                $_tmp = "$starshipCache.$PID.tmp"
                try {
                    [IO.File]::WriteAllLines($_tmp, [string[]](@($_key) + $_init))
                    [IO.File]::Move($_tmp, $starshipCache, $true)
                } catch {
                    # 置き換えに失敗しても既存 cache で起動を続け、次回起動で再試行する
                    try { [IO.File]::Delete($_tmp) } catch {}
                }
            }
        }
    } elseif ([IO.File]::Exists($starshipCache)) {
        # starship が無いのに古い cache を読むと prompt ごとに exe 不在のエラーになるので消す (簡易プロンプトに fallback)
        try { [IO.File]::Delete($starshipCache) } catch {}
    }

    # --- Dotfiles background jobs (dotcli git-prompt → sync, build --check) ---
    # dotcli を絶対 path で別 process として起動し、prompt hook が完了を見て結果を表示する。
    # $global:j = git-prompt (自動更新), $global:syncJob = sync, $global:_buildCheckJob = build --check
    $global:_dotDir = $Dot
    $global:_dotcli = (Get-Command dotcli -CommandType Application -ea 0 | select -First 1).Source
    $global:_gitPromptStamp = "$cacheDir\git-prompt.stamp"  # git-prompt の起動時に touch
    $global:_gitPromptDone = "$cacheDir\git-prompt.done"    # git-prompt の完了を hook が検知したら touch
    $global:_syncStamp = "$cacheDir\sync.stamp"
    # stamp が無いと GetLastWriteTimeUtc は 1601 年を返すので常に due
    function global:__dotcli_due($stamp, $interval) {
        [DateTime]::UtcNow - [IO.File]::GetLastWriteTimeUtc($stamp) -ge $interval
    }
    # どこかの pane の git-prompt が実行中かもしれないなら $true。
    # 起動 (stamp) より後に完了 (done) が記録されておらず、起動から猶予時間内のとき。
    # done が無いと 1601 年なので、stamp が新しい間は実行中扱い。両方無ければ stamp の経過が猶予超えで $false
    function global:__dotcli_gitPromptBusy {
        $s = [IO.File]::GetLastWriteTimeUtc($global:_gitPromptStamp)
        $s -gt [IO.File]::GetLastWriteTimeUtc($global:_gitPromptDone) -and
            [DateTime]::UtcNow - $s -lt $global:_gitPromptGrace
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
    # この pane の git-prompt ($global:j) と、別 pane で実行中かもしれない git-prompt の両方の完了後に起動して
    # pull と直列化する。待つ間は pending のまま残し、hook が毎 prompt で再確認する。
    # 間隔の判定は待った後に行う (待つ間に別 pane が sync を済ませていれば起動しない)
    function global:__dotcli_kickSync {
        if (!$global:_syncPending -or $global:j) { return }
        if (__dotcli_gitPromptBusy) { return }
        $global:_syncPending = $false
        if (__dotcli_due $global:_syncStamp $global:_syncInterval) {
            $global:syncJob = __dotcli_start @('sync', '--dot', $global:_dotDir) $global:_syncStamp
        }
    }

    # --- Prompt hook (inlined) ---
    $global:_promptHook = {
        if ($global:j -and ($r = __dotcli_end $global:j)) {
            $global:j = $null
            # 結果 (updated / dirty / error / 出力なし) によらず完了を記録し、別 pane の sync の待ちを解く
            try { [IO.File]::WriteAllBytes($global:_gitPromptDone, [byte[]]@()) } catch {}
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
                __dotcli_kickSync  # git-prompt を起動せず (間隔内 / 起動失敗)、別 pane の git-prompt も実行中でなければ即 sync
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
