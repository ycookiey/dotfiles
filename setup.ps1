$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\aliases.ps1"
$LogFile = "$HOME\.claude\setup.log"

if (!(isadmin)) { elevate "-File `"$PSCommandPath`"" }

"$(Get-Date) - Start (ScriptDir: $ScriptDir)" > $LogFile

try {
    mkd "$HOME\.config"
    mkl "$HOME\.config\wezterm" "$ScriptDir\wezterm"
    mkd "$env:APPDATA\yazi\config"
    mkl "$env:APPDATA\yazi\config" "$ScriptDir\yazi"
    mkl "$env:LOCALAPPDATA\nvim" "$ScriptDir\config\nvim"
    mkd "$env:LOCALAPPDATA\lazygit"
    mkl "$env:LOCALAPPDATA\lazygit\config.yml" "$ScriptDir\lazygit\config.yml"
    mkd "$HOME\.claude"
    mkl "$HOME\.claude\aliases.ps1" "$ScriptDir\aliases.ps1"
    mkl "$HOME\.claude\statusline.ps1" "$ScriptDir\claude\statusline.ps1"
    mkl "$HOME\.claude\settings.json" "$ScriptDir\claude\settings.json"
    mkl "$HOME\.claude\CLAUDE.md" "$ScriptDir\claude\CLAUDE.md"
    mkl "$HOME\.claude\rules" "$ScriptDir\claude\rules"
    mkl "$HOME\.claude\docs" "$ScriptDir\claude\docs"

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
    $claudeExclude = '.credentials*', '.statusline_cache', '.statusline_debug.json'
    foreach ($n in 1..3) {
        $dir = "$HOME\.claude-$n"
        mkd $dir
        gci "$HOME\.claude" -Force | ? {
            $name = $_.Name
            !($claudeExclude | ? { $name -like $_ })
        } | % {
            mkl "$dir\$($_.Name)" $_.FullName
        }
    }

    "$(Get-Date) - Done" >> $LogFile
} catch {
    "$(Get-Date) - Error: $_" >> $LogFile
}
