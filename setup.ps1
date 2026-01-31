$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$LogFile = "$HOME\.claude\setup.log"

# 管理者権限チェック・自己昇格
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Start-Process powershell -Verb RunAs -ArgumentList "-ExecutionPolicy Bypass -File `"$PSCommandPath`"" -Wait
    exit
}

"$(Get-Date) - Start (ScriptDir: $ScriptDir)" | Out-File $LogFile

try {
    ni -ItemType Directory "$HOME\.config" -Force | Out-Null
    ni -ItemType SymbolicLink -Path "$HOME\.config\wezterm" -Target "$ScriptDir\wezterm" -Force | Out-Null

    ni -ItemType Directory "$env:APPDATA\yazi\config" -Force | Out-Null
    ni -ItemType SymbolicLink -Path "$env:APPDATA\yazi\config" -Target "$ScriptDir\yazi" -Force | Out-Null

    ni -ItemType SymbolicLink -Path "$env:LOCALAPPDATA\nvim" -Target "$ScriptDir\config\nvim" -Force | Out-Null

    ni -ItemType Directory "$env:LOCALAPPDATA\lazygit" -Force | Out-Null
    ni -ItemType SymbolicLink -Path "$env:LOCALAPPDATA\lazygit\config.yml" -Target "$ScriptDir\lazygit\config.yml" -Force | Out-Null

    ni -ItemType Directory "$HOME\.claude" -Force | Out-Null
    ni -ItemType SymbolicLink -Path "$HOME\.claude\statusline.ps1" -Target "$ScriptDir\claude\statusline.ps1" -Force | Out-Null
    ni -ItemType SymbolicLink -Path "$HOME\.claude\settings.json" -Target "$ScriptDir\claude\settings.json" -Force | Out-Null
    ni -ItemType SymbolicLink -Path "$HOME\.claude\CLAUDE.md" -Target "$ScriptDir\claude\CLAUDE.md" -Force | Out-Null
    ni -ItemType SymbolicLink -Path "$HOME\.claude\rules" -Target "$ScriptDir\claude\rules" -Force | Out-Null

    "$(Get-Date) - Done" | Out-File $LogFile -Append
} catch {
    "$(Get-Date) - Error: $_" | Out-File $LogFile -Append
}
