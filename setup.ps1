$ErrorActionPreference = "Stop"

ni -ItemType Directory "$HOME\.config" -Force | Out-Null
ni -ItemType SymbolicLink -Path "$HOME\.config\wezterm" -Target "$PSScriptRoot\wezterm" -Force | Out-Null

ni -ItemType Directory "$env:APPDATA\yazi\config" -Force | Out-Null
ni -ItemType SymbolicLink -Path "$env:APPDATA\yazi\config\yazi.toml" -Target "$PSScriptRoot\yazi\yazi.toml" -Force | Out-Null
