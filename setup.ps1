$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\pwsh\aliases.ps1"
$LogFile = "$HOME\.claude\setup.log"
$LogDir = Split-Path $LogFile
if (!(Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}
"$(Get-Date) - Pre-elevation setup starting (ScriptDir: $ScriptDir)" > $LogFile

# Bitwarden secrets（別ウィンドウで対話入力を受け付ける）
$bwStarted = $false
if (gcm bw -ea 0) {
    start pwsh -Arg "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptDir\install\bw-secrets.ps1`""
    $bwStarted = $true
}

# Scoop セットアップ（未インストール時は自動、既存マシンは --scoop で手動）
if ($args -contains '--scoop' -or !(gcm scoop -ea 0)) {
    & "$ScriptDir\install\scoop.ps1"
}

# Scoop 後: 新PCでbwがインストールされた場合
if (!$bwStarted -and (gcm bw -ea 0)) {
    start pwsh -Arg "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptDir\install\bw-secrets.ps1`""
}

try {
    & "$ScriptDir\install\webinstall.ps1"
} catch {
    "$(Get-Date) - Warning: webinstall.ps1 failed: $_" >> $LogFile
}

if (!(isadmin)) {
    start pwsh -Verb RunAs -Arg "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`"" -Wait
    if (tp $LogFile) { gc $LogFile | % { wh $_ -ForegroundColor ($_ -match 'Error' ? 'Red' : 'Green') } }
    # Bitwarden secrets: 未取得があれば再実行（今度はウィンドウが埋もれない）
    if (gcm bw -ea 0) {
        & "$ScriptDir\install\bw-secrets.ps1"
    }
    exit
}

"$(Get-Date) - Start (ScriptDir: $ScriptDir)" > $LogFile

try {
    # PowerShell profile
    mkd "$HOME\Documents\PowerShell"
    mkl "$HOME\Documents\PowerShell\Microsoft.PowerShell_profile.ps1" "$ScriptDir\pwsh\profile.ps1"

    # Git
    mkl "$HOME\.gitconfig" "$ScriptDir\git\.gitconfig"
    mkl "$HOME\.gitattributes" "$ScriptDir\git\gitattributes"

    # Bash (Git Bash) — sources dotcli-generated aliases
    mkl "$HOME\.bashrc" "$ScriptDir\bash\.bashrc"

    mkd "$HOME\.config"
    mkl "$HOME\.config\wezterm" "$ScriptDir\wezterm"
    mkd "$env:APPDATA\yazi\config"
    mkl "$env:APPDATA\yazi\config" "$ScriptDir\yazi"
    mkl "$env:LOCALAPPDATA\nvim" "$ScriptDir\nvim"
    mkl "$env:APPDATA\nushell" "$ScriptDir\nushell"
    mkl "$env:APPDATA\pandoc" "$ScriptDir\pandoc"
    # Nushell: starship/zoxide キャッシュ生成
    $nuCache = "$ScriptDir\nushell\cache"
    mkd $nuCache
    if (gcm starship -ea 0) { starship init nu > "$nuCache\starship.nu" }
    if (gcm zoxide -ea 0) { zoxide init nushell > "$nuCache\zoxide.nu" }
    mkd "$env:LOCALAPPDATA\lazygit"
    mkl "$env:LOCALAPPDATA\lazygit\config.yml" "$ScriptDir\lazygit\config.yml"
    mkd "$env:LOCALAPPDATA`Low\Google\Google Japanese Input"
    mkl "$env:LOCALAPPDATA`Low\Google\Google Japanese Input\config1.db" "$ScriptDir\google-ime-config1.db"
    mkd "$HOME\.config\mise"
    mkl "$HOME\.config\mise\config.toml" "$ScriptDir\mise.toml"
    mkd "$HOME\.claude"
    mkl "$HOME\.claude\aliases.ps1" "$ScriptDir\pwsh\aliases.ps1"
    mkl "$HOME\.claude\statusline.py" "$ScriptDir\claude\statusline.py"
    mkl "$HOME\.claude\CLAUDE.md" "$ScriptDir\claude\CLAUDE.md"
    mkl "$HOME\.claude\rules" "$ScriptDir\claude\rules"
    mkl "$HOME\.claude\docs" "$ScriptDir\claude\docs"
    mkl "$HOME\.claude\agents" "$ScriptDir\claude\agents"
    mkl "$HOME\.claude\statusline-rules.toml" "$ScriptDir\claude\statusline\statusline-models.toml"
    mkl "$HOME\.claude\worktree-copy.list" "$ScriptDir\claude\worktree-copy.list"
    mkl "$HOME\.claude\hooks" "$ScriptDir\claude\hooks"
    mkd "$HOME\.claude\skills"
    mkl "$HOME\.claude\skills\urleader" "$ScriptDir\skills\urleader"

    # File association: Neovim (WezTerm)
    $wt = "$HOME\scoop\apps\wezterm\current\wezterm-gui.exe"
    if (!(tp $wt)) { $wt = "$HOME\scoop\shims\wezterm.exe" }
    if (tp $wt) {
        $cmd = "`"$wt`" start -- nvim `"%1`""
        $id = 'WezTermNvim'
        $exts = '.txt','.md','.json','.yaml','.yml','.toml','.xml','.csv','.log',
                '.ps1','.psm1','.lua','.vim','.js','.ts','.jsx','.tsx',
                '.py','.go','.rs','.c','.cpp','.h','.sh','.conf','.ini','.env'

        $null = ni "HKCU:\Software\Classes\$id\shell\open\command" -Force
        sp "HKCU:\Software\Classes\$id" '(Default)' 'Neovim (WezTerm)'
        sp "HKCU:\Software\Classes\$id\shell\open\command" '(Default)' $cmd

        foreach ($e in $exts) {
            $null = ni "HKCU:\Software\Classes\$e\OpenWithProgids" -Force
            sp "HKCU:\Software\Classes\$e\OpenWithProgids" $id '' -Type String
        }
        "$(Get-Date) - Registered WezTermNvim for $($exts.Count) extensions" >> $LogFile
    }

    # .sh file association: run with sh.exe via PATHEXT + ftype
    $shExe = "$HOME\scoop\apps\git\current\usr\bin\sh.exe"
    if (tp $shExe) {
        # PATHEXT に .SH 追加（Machine 環境変数 — 管理者ブロック内）
        $pathext = [Environment]::GetEnvironmentVariable('PATHEXT', 'Machine')
        if ($pathext -notmatch '\.SH') {
            [Environment]::SetEnvironmentVariable('PATHEXT', "$pathext;.SH", 'Machine')
            $env:PATHEXT = "$env:PATHEXT;.SH"
            "$(Get-Date) - Added .SH to PATHEXT (Machine)" >> $LogFile
        }
        # assoc + ftype（レジストリ直接書き込み）
        $null = ni "HKLM:\SOFTWARE\Classes\.sh" -Force
        sp "HKLM:\SOFTWARE\Classes\.sh" '(Default)' 'ShellScript'
        $null = ni "HKLM:\SOFTWARE\Classes\ShellScript\shell\open\command" -Force
        sp "HKLM:\SOFTWARE\Classes\ShellScript\shell\open\command" '(Default)' "`"$shExe`" `"%1`" %*"
        "$(Get-Date) - Registered .sh file association (ShellScript -> sh.exe)" >> $LogFile
    }

    # Claude Code: PATH ($HOME\.local\bin)
    $claudeBin = "$HOME\.local\bin"
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -notlike "*$claudeBin*") {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$claudeBin", 'User')
        "$(Get-Date) - Added $claudeBin to User PATH" >> $LogFile
    }
    # .local\bin は claude.exe 等のインストーラ配置物と共存するため、ディレクトリ単位ではなくファイル単位で symlink
    mkd $claudeBin
    gci "$ScriptDir\bin" -File | % { mkl "$claudeBin\$($_.Name)" $_.FullName }

    # Claude Code: WSL セットアップ + bash 切り替え
    try {
        & "$ScriptDir\install\wsl.ps1"
    } catch {
        wh "WSL セットアップでエラー: $_" -Fo Yellow
        "$(Get-Date) - WSL setup error: $_" >> $LogFile
    }

    # Claude マルチアカウント
    # settings.json / keybindings.json: Claude Code が symlink を上書き破壊するため除外（マージ型で dotcli sync が管理）
    $claudeExclude = '.credentials*', '.statusline_debug.json', 'settings.json', 'keybindings.json', '.rate-limits.json'
    foreach ($dir in gci "$HOME\.claude-*" -Dir -Force) {
        gci "$HOME\.claude" -Force | ? {
            $name = $_.Name
            !($claudeExclude | ? { $name -like $_ })
        } | % {
            $l = "$dir\$($_.Name)"
            if (tp $l) { rm $l -Force -Recurse -ea 0 }
            mkl $l $_.FullName
        }
        # skills/配下のsymlinkをミラー（個別skillが増えても自動対応）
        if (tp "$HOME\.claude\skills") {
            mkd "$dir\skills"
            gci "$HOME\.claude\skills" -Force | ? { $_.Attributes -match 'ReparsePoint' } | % {
                mkl "$dir\skills\$($_.Name)" $_.Target
            }
        }
    }

    # フォント
    & "$ScriptDir\install\fonts.ps1"
    "$(Get-Date) - Fonts install checked" >> $LogFile

    # Winget アプリ (wingetfile.json)
    & "$ScriptDir\install\winget.ps1"
    "$(Get-Date) - Winget apps install checked" >> $LogFile

    # Microsoft IME 無効化（Google IME のみ残す）
    try {
        & "$ScriptDir\install\disable-msime.ps1"
        "$(Get-Date) - Microsoft IME disabled" >> $LogFile
    } catch {
        "$(Get-Date) - Warning: disable-msime.ps1 failed: $_" >> $LogFile
    }

    # 外部skill 同期 (skills.json)
    try {
        & "$ScriptDir\install\sync-skills.ps1" -Dot $ScriptDir
        "$(Get-Date) - External skills synced" >> $LogFile
    } catch {
        "$(Get-Date) - Warning: sync-skills.ps1 failed: $_" >> $LogFile
    }

    # Rust CLIs — 初回ビルド＆エイリアス生成
    if (gcm cargo -ea 0) {
        cargo install --path "$ScriptDir\cli" --quiet
        if (gcm dotcli -ea 0) {
            dotcli build
            "$(Get-Date) - dotcli build completed" >> $LogFile
        } else {
            "$(Get-Date) - Error: dotcli initial build failed" >> $LogFile
        }
    }

    # Vivaldi 検索エンジン設定
    try {
        & "$ScriptDir\vivaldi\apply-search-engines.ps1"
        "$(Get-Date) - Vivaldi search engine configured" >> $LogFile
    } catch {
        "$(Get-Date) - Warning: Vivaldi search engine setup skipped: $_" >> $LogFile
    }

    # Startup manager (TaskScheduler)
    & "$ScriptDir\startup\register.ps1" -Action Register
    "$(Get-Date) - Startup registered" >> $LogFile

    # 同期（settings.json マージ・scoopfile・wingetfile・MCP servers 等）
    if (gcm dotcli -ea 0) {
        dotcli sync --dot $ScriptDir | Out-Null
        "$(Get-Date) - Sync completed" >> $LogFile
    } else {
        "$(Get-Date) - Sync skipped: dotcli not found" >> $LogFile
    }

    "$(Get-Date) - Done" >> $LogFile
} catch {
    "$(Get-Date) - Error: $_" >> $LogFile
}
