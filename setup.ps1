$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\pwsh\aliases.ps1"
$Host.UI.RawUI.WindowTitle = if (isadmin) { 'dotfiles setup (admin)' } else { 'dotfiles setup (main)' }
$LogFile = "$HOME\.claude\setup.log"
$LogDir = Split-Path $LogFile
if (!(Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

# --elevated: 非管理者側が起動した管理者プロセス。ログ初期化・bw・scoop・webinstall は非管理者側で済んでいるので飛ばす
# (飛ばさないと非管理者側のログ、例えば webinstall 失敗の警告が消える)
$elevated = $args -contains '--elevated'

if (!$elevated) {
    # ログの初期化 (切り詰め) はここ 1 か所だけ
    "$(Get-Date) - Pre-elevation setup starting (ScriptDir: $ScriptDir)" > $LogFile

    # Bitwarden secrets（別ウィンドウで対話入力を受け付ける）
    $skipBw = $args -contains '--skip-bw'
    $bwStarted = $false
    if (!$skipBw -and (gcm bw -ea 0)) {
        start pwsh -Arg "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptDir\install\bw-secrets.ps1`""
        $bwStarted = $true
    }

    # Scoop セットアップ（未インストール時は自動、既存マシンは --scoop で手動）
    if ($args -contains '--scoop' -or !(gcm scoop -ea 0)) {
        & "$ScriptDir\install\scoop.ps1"
        # 失敗したアプリは scoop.ps1 が一覧を出し exit 1 で返す。後続 (管理者側の設定) は続ける
        if ($LASTEXITCODE) {
            "$(Get-Date) - Warning: scoop.ps1 exit $LASTEXITCODE (failed apps listed above; rerun install\scoop.ps1)" >> $LogFile
        }
    }

    # Scoop 後: 新PCでbwがインストールされた場合
    if (!$skipBw -and !$bwStarted -and (gcm bw -ea 0)) {
        start pwsh -Arg "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptDir\install\bw-secrets.ps1`""
    }

    try {
        & "$ScriptDir\install\webinstall.ps1"
    } catch {
        "$(Get-Date) - Warning: webinstall.ps1 failed: $_" >> $LogFile
    }
}

if (!(isadmin)) {
    wh ""
    wh "==> 管理者ウィンドウ起動中 (UAC承認後、別ウィンドウで処理)..." -ForegroundColor Cyan
    wh "    完了するまでこのタブは待機します" -ForegroundColor DarkGray
    # 管理者側の exit code (失敗した手順があれば 1) をこのプロセスの exit code にする (bootstrap.ps1 Phase 2 が見る)
    $exitCode = 1
    try {
        $p = start pwsh -Verb RunAs -Arg "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" --skip-bw --elevated" -Wait -PassThru
        if ($null -ne $p.ExitCode) { $exitCode = $p.ExitCode }
    } catch {
        # UAC 拒否などで起動できなかった場合
        "$(Get-Date) - Error: admin setup did not run or report an exit code: $_" >> $LogFile
    }
    wh ""
    wh "==> 管理者ウィンドウ終了。ログ表示:" -ForegroundColor Cyan
    if (tp $LogFile) { gc $LogFile | % { wh $_ -ForegroundColor ($_ -match 'Error' ? 'Red' : 'Green') } }
    # Bitwarden secrets: 未取得があれば再実行（今度はウィンドウが埋もれない）
    if (gcm bw -ea 0) {
        wh ""
        wh "==> Bitwarden secrets 仕上げ取得 (未取得分のみ)..." -ForegroundColor Cyan
        # bw-secrets.ps1 は認証失敗を exit 1 で返す (& 呼び出しでは throw しない)。throw と exit code の両方を見る
        $bwError = $null
        $global:LASTEXITCODE = 0
        try {
            & "$ScriptDir\install\bw-secrets.ps1"
            if ($global:LASTEXITCODE) { $bwError = "exit $global:LASTEXITCODE" }
        } catch {
            $bwError = "$_"
        }
        if ($bwError) {
            wh "Bitwarden secrets 取得でエラー: $bwError" -Fo Red
            "$(Get-Date) - Error: bw-secrets.ps1 failed: $bwError" >> $LogFile
            if (!$exitCode) { $exitCode = 1 }
        }
    }
    wh ""
    if ($exitCode) {
        wh "==> setup.ps1 失敗あり (exit $exitCode)。上のログの Error 行を参照" -ForegroundColor Red
    } else {
        wh "==> setup.ps1 完了" -ForegroundColor Green
    }
    exit $exitCode
}

"$(Get-Date) - Start (ScriptDir: $ScriptDir)" >> $LogFile

# 各手順は互いの前提ではないので独立させる: 1 つが失敗しても後続を続け、最後に失敗一覧を出して exit 1
# 前提ツールが無いだけの手順は Invoke-Step の外で判定し、失敗ではなく Skipped として記録する
$failedSteps = [Collections.Generic.List[string]]::new()
function Invoke-Step([string]$Name, [scriptblock]$Body, [string]$Done) {
    try {
        & $Body
        "$(Get-Date) - $($Done ? $Done : "${Name}: ok")" >> $LogFile
    } catch {
        $script:failedSteps.Add($Name)
        wh "Error: ${Name}: $_" -Fo Red
        "$(Get-Date) - Error: $Name failed: $_" >> $LogFile
    }
}
# native コマンド (と exit で終わる .ps1) の直後に呼ぶ。非 0 なら throw し、Invoke-Step が失敗として拾う
function Assert-ExitCode([string]$What) {
    if ($global:LASTEXITCODE) { throw "$What exit $global:LASTEXITCODE" }
}
function Write-Skip([string]$Message) { "$(Get-Date) - Skipped: $Message" >> $LogFile }
# registry の New-Item -Force は既存キーを作り直して値・サブキーを消すので、無いときだけ作る
function Initialize-RegistryKey([string]$Path) {
    if (!(tp $Path)) { $null = ni $Path -Force }
}

Invoke-Step 'PowerShell profile' {
    mkd "$HOME\Documents\PowerShell"
    mkl "$HOME\Documents\PowerShell\Microsoft.PowerShell_profile.ps1" "$ScriptDir\pwsh\profile.ps1"
}

Invoke-Step 'Git config' {
    mkl "$HOME\.gitconfig" "$ScriptDir\git\.gitconfig"
    # git/.gitconfig の core.attributesFile / core.excludesFile がこの 2 つを指す (XDG の ~/.config/git/ は読まれない)
    mkl "$HOME\.gitattributes" "$ScriptDir\git\gitattributes"
    mkl "$HOME\.gitignore_global" "$ScriptDir\git\ignore"
}

# SSH config（鍵自体は install/bw-secrets.ps1 が Bitwarden から取得）
Invoke-Step 'SSH config' {
    mkd "$HOME\.ssh"
    mkl "$HOME\.ssh\config" "$ScriptDir\ssh\config"
}

# GitHub host key: 初回接続時のyes/no確認を避けるためfingerprint検証付きで known_hosts に事前登録
# 公式fingerprint: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints
Invoke-Step 'GitHub known_hosts' {
    $knownHosts = "$HOME\.ssh\known_hosts"
    $hasGithub = (tp $knownHosts) -and (ssh-keygen -F github.com -f $knownHosts 2>$null)
    if (!$hasGithub) {
        $expectedFp = 'SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU'
        $line = ssh-keyscan -t ed25519 github.com 2>$null
        if ($line) {
            $actualFp = (($line | ssh-keygen -lf -) -split ' ')[1]
            if ($actualFp -ne $expectedFp) {
                throw "GitHub host key fingerprint mismatch (got: $actualFp, expected: $expectedFp)"
            }
            if (!(tp $knownHosts)) { ni $knownHosts -ItemType File | Out-Null }
            ac $knownHosts $line
            "$(Get-Date) - Added GitHub host key to known_hosts" >> $LogFile
        } else {
            "$(Get-Date) - Warning: ssh-keyscan github.com failed" >> $LogFile
        }
    }
}

# ssh-agent: 自動起動化 + 鍵登録（passphraseなし鍵を非対話で追加）
$sshAgent = Get-Service ssh-agent -ea 0
if ($sshAgent) {
    Invoke-Step 'ssh-agent' {
        if ($sshAgent.StartType -ne 'Automatic') {
            Set-Service ssh-agent -StartupType Automatic
        }
        if ($sshAgent.Status -ne 'Running') {
            Start-Service ssh-agent
        }
        $keyPath = "$HOME\.ssh\id_ed25519"
        if (tp $keyPath) {
            $fp = ((ssh-keygen -lf $keyPath 2>$null) -split ' ')[1]
            $registered = $fp -and ((ssh-add -l 2>$null) -match [regex]::Escape($fp))
            if (!$registered) {
                ssh-add $keyPath 2>&1 | Out-Null
                "$(Get-Date) - ssh-add: $keyPath" >> $LogFile
            }
        }
    }
} else {
    Write-Skip 'ssh-agent (service not found)'
}

Invoke-Step 'Tool config links' {
    # Bash (Git Bash) — sources dotcli-generated aliases
    mkl "$HOME\.bashrc" "$ScriptDir\bash\.bashrc"

    # WSL 2: memory の上限と cache の返し方(claude/docs/wsl.md)
    mkl "$HOME\.wslconfig" "$ScriptDir\wsl\wslconfig"

    mkd "$HOME\.config"
    mkl "$HOME\.config\wezterm" "$ScriptDir\wezterm"
    mkd "$env:APPDATA\yazi\config"
    mkl "$env:APPDATA\yazi\config" "$ScriptDir\yazi"
    mkl "$env:LOCALAPPDATA\nvim" "$ScriptDir\nvim"
    mkl "$env:APPDATA\nushell" "$ScriptDir\nushell"
    mkl "$env:APPDATA\pandoc" "$ScriptDir\pandoc"
    mkd "$env:LOCALAPPDATA\lazygit"
    mkl "$env:LOCALAPPDATA\lazygit\config.yml" "$ScriptDir\lazygit\config.yml"
    mkd "$env:LOCALAPPDATA`Low\Google\Google Japanese Input"
    mkl "$env:LOCALAPPDATA`Low\Google\Google Japanese Input\config1.db" "$ScriptDir\google-ime-config1.db"
    mkd "$HOME\.config\mise"
    mkl "$HOME\.config\mise\config.toml" "$ScriptDir\mise.toml"
}

# Nushell: starship/zoxide キャッシュ生成は generate-cache.nu に一本化 (exe パス置換等の後処理を通す)
# 出力先は $nu.default-config-dir\cache (= 上の mkl で張った $ScriptDir\nushell\cache)
if (gcm nu -ea 0) {
    Invoke-Step 'Nushell caches' {
        nu "$ScriptDir\nushell\generate-cache.nu" --force
        Assert-ExitCode 'generate-cache.nu'
    }
} else {
    Write-Skip 'Nushell caches (nu not found)'
}

Invoke-Step 'Claude config links' {
    mkd "$HOME\.claude"
    mkl "$HOME\.claude\aliases.ps1" "$ScriptDir\pwsh\aliases.ps1"
    mkl "$HOME\.claude\CLAUDE.md" "$ScriptDir\claude\CLAUDE.md"
    mkl "$HOME\.claude\docs" "$ScriptDir\claude\docs"
    mkl "$HOME\.claude\agents" "$ScriptDir\claude\agents"
    mkl "$HOME\.claude\statusline-rules.toml" "$ScriptDir\claude\statusline\statusline-models.toml"
    mkl "$HOME\.claude\worktree-copy.list" "$ScriptDir\claude\worktree-copy.list"
    mkl "$HOME\.claude\hooks" "$ScriptDir\claude\hooks"
    mkd "$HOME\.claude\skills"
    mkl "$HOME\.claude\skills\urleader" "$ScriptDir\skills\urleader"
}

# File association: Neovim (WezTerm)
# GUI の exe を優先 (shim はコンソール経由で開く)
$scoopRoot = $env:SCOOP ? $env:SCOOP : "$HOME\scoop"
$wt = "$scoopRoot\apps\wezterm-nightly\current\wezterm-gui.exe", "$scoopRoot\apps\wezterm\current\wezterm-gui.exe" |
    ? { tp $_ } | select -First 1
if (!$wt) { $wt = (gcm wezterm-gui -CommandType Application -ea 0 | select -First 1).Source }
if (!$wt) { $wt = "$scoopRoot\shims\wezterm.exe" }
if (tp $wt) {
    Invoke-Step 'Neovim file association' {
        $cmd = "`"$wt`" start -- nvim `"%1`""
        $id = 'WezTermNvim'
        $exts = '.txt','.md','.json','.yaml','.yml','.toml','.xml','.csv','.log',
                '.ps1','.psm1','.lua','.vim','.js','.ts','.jsx','.tsx',
                '.py','.go','.rs','.c','.cpp','.h','.sh','.conf','.ini','.env'

        Initialize-RegistryKey "HKCU:\Software\Classes\$id\shell\open\command"
        sp "HKCU:\Software\Classes\$id" '(Default)' 'Neovim (WezTerm)'
        sp "HKCU:\Software\Classes\$id\shell\open\command" '(Default)' $cmd

        foreach ($e in $exts) {
            Initialize-RegistryKey "HKCU:\Software\Classes\$e\OpenWithProgids"
            sp "HKCU:\Software\Classes\$e\OpenWithProgids" $id '' -Type String
        }
        "$(Get-Date) - Registered WezTermNvim for $($exts.Count) extensions" >> $LogFile
    }
} else {
    Write-Skip 'Neovim file association (wezterm not found)'
}

# .sh file association: run with sh.exe via PATHEXT + ftype
$shExe = "$HOME\scoop\apps\git\current\usr\bin\sh.exe"
if (tp $shExe) {
    Invoke-Step '.sh file association' {
        # PATHEXT に .SH 追加（Machine 環境変数 — 管理者ブロック内）
        $pathext = [Environment]::GetEnvironmentVariable('PATHEXT', 'Machine')
        if ($pathext -notmatch '\.SH') {
            [Environment]::SetEnvironmentVariable('PATHEXT', "$pathext;.SH", 'Machine')
            $env:PATHEXT = "$env:PATHEXT;.SH"
            "$(Get-Date) - Added .SH to PATHEXT (Machine)" >> $LogFile
        }
        # assoc + ftype（レジストリ直接書き込み）
        Initialize-RegistryKey "HKLM:\SOFTWARE\Classes\.sh"
        sp "HKLM:\SOFTWARE\Classes\.sh" '(Default)' 'ShellScript'
        Initialize-RegistryKey "HKLM:\SOFTWARE\Classes\ShellScript\shell\open\command"
        sp "HKLM:\SOFTWARE\Classes\ShellScript\shell\open\command" '(Default)' "`"$shExe`" `"%1`" %*"
        "$(Get-Date) - Registered .sh file association (ShellScript -> sh.exe)" >> $LogFile
    }
} else {
    Write-Skip '.sh file association (Git sh.exe not found)'
}

# Claude Code: PATH ($HOME\.local\bin)
$claudeBin = "$HOME\.local\bin"
Invoke-Step 'Claude PATH' {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -notlike "*$claudeBin*") {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$claudeBin", 'User')
        "$(Get-Date) - Added $claudeBin to User PATH" >> $LogFile
    }
}
# .local\bin は claude.exe 等のインストーラ配置物と共存するため、ディレクトリ単位ではなくファイル単位で symlink
Invoke-Step 'bin links' {
    mkd $claudeBin
    gci "$ScriptDir\bin" -File | % { mkl "$claudeBin\$($_.Name)" $_.FullName }
}

# Claude Code: WSL セットアップ + bash 切り替え
Invoke-Step 'WSL' { & "$ScriptDir\install\wsl.ps1" }

# Claude マルチアカウント
Invoke-Step 'Claude multi-account' {
    # ~/.claude の entry のうち、下の許可リストにある名前だけを各アカウント (~/.claude-<数字>) へ link する。
    # 新しい entry は、ここに足さないと共有されない。Claude Code がアカウントごとに持つ実行時の状態
    # (daemon/, jobs/, state/, logs/, daemon.log, .last-cleanup, *.bak 等) や *.log は入れない
    # 設定・skill・docs (dotfiles 管理物と、dotcli sync が keybindings の前回値を置く .dotcli-keybindings-prev.json)
    $claudeSharedConfig = 'CLAUDE.md', 'agents', 'docs', 'hooks', 'skills', 'plugins', 'aliases.ps1', 'statusline-rules.toml', 'statusline.ps1', 'statusline.sh', 'worktree-copy.list', '.mcp.json', '.dotcli-keybindings-prev.json'
    # 履歴と session: アカウントを替えても resume できるように共有する (dotcli resume / token-audit も ~/.claude/projects を読む)
    $claudeSharedHistory = 'projects', 'sessions', 'history.jsonl', 'file-history', 'session-env', 'shell-snapshots', 'todos', 'tasks', 'plans', 'teams'
    # cache (.glm-rate-limits.json は claude-statusline が全アカウント共通の cache として上書きする)
    $claudeSharedCache = 'cache', 'paste-cache', 'db', 'ide', 'debug', 'backups', 'statsig', 'telemetry', 'usage-data', 'stats-cache.json', 'mcp-needs-auth-cache.json', '.statusline_cache', '.glm-rate-limits.json'
    # 認証補助と API key、アカウント切替 (.last_account / .recommended は dotcli c と ycusage が使う)
    $claudeSharedAuth = '.glm-api-key', '.tavily-api-key', '.oauth-verifier', '.oauth-verifier-3', '.last_account', '.recommended'
    # 許可リストに入れてはいけない名前。settings.json / keybindings.json: Claude Code が symlink を上書き破壊するため
    # (マージ型で dotcli sync が管理)。.credentials* 等はアカウント固有。tests/test-install.ps1 が許可リストとの重なりを検査する
    $claudeExclude = '.credentials*', '.statusline_debug.json', 'settings.json', 'keybindings.json', '.rate-limits.json'
    # 許可リストから外した名前の既存 link は消さない (アカウントに残っていても害が無く、消すと使用中のセッションが困りうる)
    $shared = foreach ($name in $claudeSharedConfig + $claudeSharedHistory + $claudeSharedCache + $claudeSharedAuth) {
        if ($claudeExclude | ? { $name -like $_ }) {
            "$(Get-Date) - Warning: skip mirror (excluded name in allowlist): $name" >> $LogFile
            continue
        }
        # 元が無い名前は link しない (リンク切れを作らない)
        $src = gi -LiteralPath (Join-Path "$HOME\.claude" $name) -Force -ea 0
        if ($src) { $src }
    }
    # アカウントディレクトリは .claude-<数字> だけ (.claude-browsers や .claude-1.lock は別物)
    foreach ($dir in gci $HOME -Dir -Force | ? Name -match '^\.claude-\d+$') {
        foreach ($src in $shared) {
            $l = Join-Path $dir.FullName $src.Name
            # Test-Path はリンク切れを「無い」と答えるので Get-Item で見る
            $item = gi -LiteralPath $l -Force -ea 0
            if ($item) {
                # 実体 (アカウント固有のデータ) は消さない
                if ($item.Attributes -notmatch 'ReparsePoint') {
                    "$(Get-Date) - Warning: skip mirror (real entry): $l" >> $LogFile
                    continue
                }
                if ("$($item.Target)".TrimEnd('\') -eq $src.FullName.TrimEnd('\')) { continue }
                # リンク自体だけを消す (-Recurse はリンク先をたどって中身を消しうる)
                $item.Delete()
            }
            mkl $l $src.FullName
        }
    }
}

# フォント
Invoke-Step 'Fonts' { & "$ScriptDir\install\fonts.ps1" } 'Fonts install checked'

# Winget アプリ (wingetfile.json)
# winget.ps1 はアプリの失敗を一覧と再実行コマンドを出して exit 1 で返す。他の手順と同じく失敗として数える
# (導入済み・再起動で完了する戻り値は winget.ps1 側で失敗に数えない)
# (setup が exit 1 になり、bootstrap は Phase 2 の失敗として最後の一覧に載せて後続 Phase を続ける)
Invoke-Step 'Winget apps' {
    & "$ScriptDir\install\winget.ps1"
    if ($global:LASTEXITCODE) {
        throw "install\winget.ps1 exit $global:LASTEXITCODE (failed apps listed above; rerun install\winget.ps1)"
    }
} 'Winget apps install checked'

# python3: Windows の Python は python.exe しか置かないので、`#!/usr/bin/env python3` で始まる script
# (git filter-repo 等) が python3 を見つけられず失敗する。Python Launcher (wingetfile.json) の最新の 3.x を
# python3 として起動する scoop の shim を作る (WindowsApps の python3 より scoop の shims が PATH の前にある)
$pyLauncher = "$env:WINDIR\py.exe"
if (tp "$scoopRoot\shims\python3.shim") {
    "$(Get-Date) - python3 shim: already there" >> $LogFile
} elseif (!(tp $pyLauncher)) {
    Write-Skip "python3 shim ($pyLauncher not found; install Python.Launcher and rerun: scoop shim add python3 $pyLauncher '--' '-3')"
} else {
    Invoke-Step 'python3 shim' {
        # winget.ps1 の終了コードが残っていると Assert-ExitCode が誤って失敗にする
        $global:LASTEXITCODE = 0
        scoop shim add python3 $pyLauncher '--' '-3'
        Assert-ExitCode 'scoop shim add python3'
    } 'python3 shim added (py -3)'
}

# Microsoft IME 無効化（Google IME のみ残す）
Invoke-Step 'Disable Microsoft IME' { & "$ScriptDir\install\disable-msime.ps1" } 'Microsoft IME disabled'

# 外部skill 同期 (skills.json)
# gh api は未ログインだと失敗する。bootstrap 直後 (gh 未ログイン) を失敗扱いにしないよう前提として判定
$ghReady = $false
if (gcm gh -ea 0) {
    gh auth status --active --hostname github.com *>$null
    $ghReady = !$LASTEXITCODE
}
if ($ghReady) {
    Invoke-Step 'External skills' {
        & "$ScriptDir\install\sync-external-skills.ps1" -Dot $ScriptDir
    } 'External skills synced'
} else {
    Write-Skip 'External skills (gh not found or not logged in to github.com)'
}

# Rust CLIs — 初回ビルド＆エイリアス生成
if (gcm cargo -ea 0) {
    Invoke-Step 'dotcli build' {
        # cargo install は dotcli が無い初回だけ。実行中の dotcli を上書きすると move が os error 5 で失敗するため、
        # 2 回目以降は dotcli build (rename してから copy する安全な置き換え) だけを使う
        if (!(gcm dotcli -ea 0)) {
            cargo install --path "$ScriptDir\cli" --quiet
            Assert-ExitCode 'cargo install'
            if (!(gcm dotcli -ea 0)) { throw 'dotcli not found on PATH after cargo install' }
        }
        dotcli build
        Assert-ExitCode 'dotcli build'
    } 'dotcli build completed'
} else {
    Write-Skip 'dotcli build (cargo not found)'
}

# Vivaldi 検索エンジン設定
# apply-search-engines.ps1 は Preferences が無いと throw する。Vivaldi 未起動の新PCを失敗扱いにしないよう前提として判定
# TODO: apply-search-engines.ps1 側で Preferences 不在を throw でなく return にし、この判定を消す
# 起動中 (Preferences を書き戻されるので適用しない) と sqlite3 不在 (scoopfile に無く新PCでは無い) も
# スクリプトの挙動に頼らず前提として判定し、未適用を configured や失敗として記録しない
# TODO: 判定後〜スクリプト実行までに Vivaldi が起動されると、スクリプトが黙って return した場合 configured と記録される
$vivaldiPrefs = "$HOME\scoop\apps\vivaldi\current\User Data\Default\Preferences"
if (!(tp $vivaldiPrefs)) {
    Write-Skip "Vivaldi search engines (profile not found: $vivaldiPrefs)"
} elseif (Get-Process vivaldi -ea 0) {
    Write-Skip 'Vivaldi search engines (Vivaldi is running; close it and rerun vivaldi\apply-search-engines.ps1)'
} elseif (!(gcm sqlite3 -ea 0)) {
    Write-Skip 'Vivaldi search engines (sqlite3 not found; run scoop install sqlite and rerun vivaldi\apply-search-engines.ps1)'
} else {
    Invoke-Step 'Vivaldi search engines' {
        & "$ScriptDir\vivaldi\apply-search-engines.ps1"
    } 'Vivaldi search engine configured'
}

# Startup manager (TaskScheduler)
Invoke-Step 'Startup manager' {
    # register.ps1 は失敗を exit 1 で返す (& 呼び出しでは throw しない)
    $global:LASTEXITCODE = 0
    & "$ScriptDir\startup\register.ps1" -Action Register
    Assert-ExitCode 'startup/register.ps1'
} 'Startup registered'

# Neovim プラグイン: lazy-lock.json から install + build (telescope-fzf-native は CMake + MSVC Build Tools で libfzf.dll を生成)
if (gcm nvim -ea 0) {
    Invoke-Step 'Neovim plugins' {
        nvim --headless "+Lazy! restore" +qa *>> $LogFile
        Assert-ExitCode 'nvim Lazy! restore'
    } 'Neovim plugins restored'
} else {
    Write-Skip 'Neovim plugins (nvim not found)'
}

# 同期（settings.json マージ・scoopfile・wingetfile・MCP servers 等）
if (gcm dotcli -ea 0) {
    Invoke-Step 'dotcli sync' {
        dotcli sync --dot $ScriptDir | Out-Null
        Assert-ExitCode 'dotcli sync'
    } 'Sync completed'
} else {
    "$(Get-Date) - Sync skipped: dotcli not found" >> $LogFile
}

if ($failedSteps.Count) {
    $summary = "Error: $($failedSteps.Count) step(s) failed: $($failedSteps -join ', ')"
    "$(Get-Date) - $summary" >> $LogFile
    wh $summary -Fo Red
    exit 1
}
"$(Get-Date) - Done" >> $LogFile
