$ErrorActionPreference = "Stop"

ni -ItemType Directory "$HOME\.config" -Force | Out-Null
ni -ItemType SymbolicLink -Path "$HOME\.config\wezterm" -Target "$PSScriptRoot\wezterm" -Force | Out-Null
