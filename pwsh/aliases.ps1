Set-Alias -Name tp -Value Test-Path  -Force -ErrorAction SilentlyContinue
Set-Alias -Name sc -Value Set-Content -Force -ErrorAction SilentlyContinue
function wh { Write-Host @args }
function mkd($p) { [void](ni -I Directory $p -Force) }
function mkl($p,$t) { [void](ni -I SymbolicLink $p -Target $t -Force) }
function isadmin { ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) }
function elevate($args_str) { start pwsh -Verb RunAs -Arg "-ExecutionPolicy Bypass $args_str" -Wait; exit }
function Start-App($Name) {
    explorer "shell:AppsFolder\$((Get-StartApps $Name | select -f 1).AppID)"
}
