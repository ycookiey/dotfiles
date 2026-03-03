$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\pwsh\aliases.ps1"
$LogFile = "$HOME\.claude\setup.log"

# Scoop セットアップ（未インストール時は自動、既存マシンは --scoop で手動）
if ($args -contains '--scoop' -or !(gcm scoop -ea 0)) {
    & "$ScriptDir\install\scoop.ps1"
}

if (!(isadmin)) {
    start pwsh -Verb RunAs -Arg "-ExecutionPolicy Bypass -File `"$PSCommandPath`"" -Wait
    if (tp $LogFile) { gc $LogFile | % { wh $_ -ForegroundColor ($_ -match 'Error' ? 'Red' : 'Green') } }
    exit
}

"$(Get-Date) - Start (ScriptDir: $ScriptDir)" > $LogFile

try {
    mkd "$HOME\.config"
    mkl "$HOME\.config\wezterm" "$ScriptDir\wezterm"
    mkd "$env:APPDATA\yazi\config"
    mkl "$env:APPDATA\yazi\config" "$ScriptDir\yazi"
    mkl "$env:LOCALAPPDATA\nvim" "$ScriptDir\nvim"
    mkl "$env:APPDATA\nushell" "$ScriptDir\nushell"
    mkd "$env:LOCALAPPDATA\lazygit"
    mkl "$env:LOCALAPPDATA\lazygit\config.yml" "$ScriptDir\lazygit\config.yml"
    mkd "$HOME\.claude"
    mkl "$HOME\.claude\aliases.ps1" "$ScriptDir\pwsh\aliases.ps1"
    mkl "$HOME\.claude\statusline.ps1" "$ScriptDir\claude\statusline.ps1"
    mkl "$HOME\.claude\CLAUDE.md" "$ScriptDir\claude\CLAUDE.md"
    mkl "$HOME\.claude\rules" "$ScriptDir\claude\rules"
    mkl "$HOME\.claude\docs" "$ScriptDir\claude\docs"
    mkl "$HOME\.claude\skills" "$ScriptDir\claude\skills"
    mkl "$ScriptDir\claude\skills\life" "C:\Main\Project\life\skills\life"

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
