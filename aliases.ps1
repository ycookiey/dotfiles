sal tp Test-Path
sal sc Set-Content
function wh { Write-Host @args }
function mkd($p) { [void](ni -I Directory $p -Force) }
function mkl($p,$t) { [void](ni -I SymbolicLink $p -Target $t -Force) }
function isadmin { ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) }
function elevate($args_str) { start pwsh -Verb RunAs -Arg "-ExecutionPolicy Bypass $args_str" -Wait; exit }
function Start-App($Name) {
    explorer "shell:AppsFolder\$((Get-StartApps $Name | select -f 1).AppID)"
}
function htb { start "C:\Main\Project\HideTaskBar-temp\src\HideTaskBar\bin\Debug\net8.0-windows\HideTaskBar.exe" }
function lghub { start "$env:ProgramFiles\LGHUB\system_tray\lghub_system_tray.exe" -Arg "--minimized" }
function nvbc { start "$env:ProgramFiles\NVIDIA Corporation\NVIDIA Broadcast\NVIDIA Broadcast.exe" -Arg '--process-start-args "--launch-hidden"' }
function kindle { explorer kindle: }
function dis { explorer discord: }
function slk { explorer slack: }
function obsd { obsidian }
function cal { Start-App "Notion Calendar" }
function viv { vivaldi }
function rmt { explorer parsec: }
function spotify { explorer spotify: }
