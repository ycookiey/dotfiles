$ErrorActionPreference = "Stop"

ni -ItemType Directory "$HOME\.config" -Force | Out-Null
ni -ItemType SymbolicLink -Path "$HOME\.config\wezterm" -Target "$PSScriptRoot\wezterm" -Force | Out-Null

ni -ItemType Directory "$env:APPDATA\yazi\config" -Force | Out-Null
ni -ItemType SymbolicLink -Path "$env:APPDATA\yazi\config" -Target "$PSScriptRoot\yazi" -Force | Out-Null

ni -ItemType SymbolicLink -Path "$env:LOCALAPPDATA\nvim" -Target "$PSScriptRoot\config\nvim" -Force | Out-Null

ni -ItemType Directory "$env:LOCALAPPDATA\lazygit" -Force | Out-Null
ni -ItemType SymbolicLink -Path "$env:LOCALAPPDATA\lazygit\config.yml" -Target "$PSScriptRoot\lazygit\config.yml" -Force | Out-Null

ni -ItemType Directory "$HOME\.claude" -Force | Out-Null
ni -ItemType SymbolicLink -Path "$HOME\.claude\statusline.ps1" -Target "$PSScriptRoot\bin\claude-statusline.ps1" -Force | Out-Null
ni -ItemType SymbolicLink -Path "$HOME\.claude\settings.json" -Target "$PSScriptRoot\claude\settings.json" -Force | Out-Null
