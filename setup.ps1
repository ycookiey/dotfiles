$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\pwsh\aliases.ps1"
$LogFile = "$HOME\.claude\setup.log"
$LogDir = Split-Path $LogFile
if (!(Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}
"$(Get-Date) - Pre-elevation setup starting (ScriptDir: $ScriptDir)" > $LogFile

# Scoop セットアップ（未インストール時は自動、既存マシンは --scoop で手動）
if ($args -contains '--scoop' -or !(gcm scoop -ea 0)) {
    & "$ScriptDir\install\scoop.ps1"
}

try {
    & "$ScriptDir\install\webinstall.ps1"
} catch {
    "$(Get-Date) - Warning: webinstall.ps1 failed: $_" >> $LogFile
}

if (!(isadmin)) {
    start pwsh -Verb RunAs -Arg "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`"" -Wait
    if (tp $LogFile) { gc $LogFile | % { wh $_ -ForegroundColor ($_ -match 'Error' ? 'Red' : 'Green') } }
    exit
}

"$(Get-Date) - Start (ScriptDir: $ScriptDir)" > $LogFile

try {
    # PowerShell profile
    mkd "$HOME\Documents\PowerShell"
    mkl "$HOME\Documents\PowerShell\Microsoft.PowerShell_profile.ps1" "$ScriptDir\pwsh\profile.ps1"

    mkd "$HOME\.config"
    mkl "$HOME\.config\wezterm" "$ScriptDir\wezterm"
    mkd "$env:APPDATA\yazi\config"
    mkl "$env:APPDATA\yazi\config" "$ScriptDir\yazi"
    mkl "$env:LOCALAPPDATA\nvim" "$ScriptDir\nvim"
    mkl "$env:APPDATA\nushell" "$ScriptDir\nushell"
    mkd "$env:LOCALAPPDATA\lazygit"
    mkl "$env:LOCALAPPDATA\lazygit\config.yml" "$ScriptDir\lazygit\config.yml"
    mkd "$HOME\.config\mise"
    mkl "$HOME\.config\mise\config.toml" "$ScriptDir\mise.toml"
    mkd "$HOME\.claude"
    mkl "$HOME\.claude\aliases.ps1" "$ScriptDir\pwsh\aliases.ps1"
    mkl "$HOME\.claude\statusline.ps1" "$ScriptDir\claude\statusline.ps1"
    mkl "$HOME\.claude\CLAUDE.md" "$ScriptDir\claude\CLAUDE.md"
    mkl "$HOME\.claude\rules" "$ScriptDir\claude\rules"
    mkl "$HOME\.claude\docs" "$ScriptDir\claude\docs"
    mkd "$HOME\.claude\skills"

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

    # Claude Code: PATH ($HOME\.local\bin)
    $claudeBin = "$HOME\.local\bin"
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -notlike "*$claudeBin*") {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$claudeBin", 'User')
        "$(Get-Date) - Added $claudeBin to User PATH" >> $LogFile
    }

    # Claude Code: WSL セットアップ + bash 切り替え
    try {
        & "$ScriptDir\install\wsl.ps1"
    } catch {
        wh "WSL セットアップでエラー: $_" -Fo Yellow
        "$(Get-Date) - WSL setup error: $_" >> $LogFile
    }

    # Claude マルチアカウント
    $claudeExclude = '.credentials*', '.statusline_cache', '.statusline_debug.json', 'settings.json'
    foreach ($dir in gci "$HOME\.claude-*" -Dir -Force) {
        gci "$HOME\.claude" -Force | ? {
            $name = $_.Name
            !($claudeExclude | ? { $name -like $_ })
        } | % {
            $l = "$dir\$($_.Name)"
            if (tp $l) { rm $l -Force -Recurse -ea 0 }
            mkl $l $_.FullName
        }
    }

    # フォント
    & "$ScriptDir\install\fonts.ps1"
    "$(Get-Date) - Fonts install checked" >> $LogFile

    # Winget アプリ (wingetfile.json)
    & "$ScriptDir\install\winget.ps1"
    "$(Get-Date) - Winget apps install checked" >> $LogFile

    # dotcli (Rust CLI) — ビルド＆エイリアス生成
    if (gcm cargo -ea 0) {
        cargo install --path "$ScriptDir\cli" --quiet 2>$null
        dotcli generate -o $ScriptDir
        "$(Get-Date) - dotcli installed and generated" >> $LogFile
    }

    # Startup manager (TaskScheduler)
    & "$ScriptDir\startup\register.ps1" -Action Register
    "$(Get-Date) - Startup registered" >> $LogFile

    "$(Get-Date) - Done" >> $LogFile
} catch {
    "$(Get-Date) - Error: $_" >> $LogFile
}
