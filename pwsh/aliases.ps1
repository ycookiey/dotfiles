Set-Alias -Name tp -Value Test-Path -Force -ErrorAction SilentlyContinue
Set-Alias -Name sc -Value Set-Content -Force -ErrorAction SilentlyContinue
function wh { Write-Host @args }
function mkd($p) { [void](ni -I Directory $p -Force) }
function mkl($p,$t) { [void](ni -I SymbolicLink $p -Target $t -Force) }
function isadmin { ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) }
function elevate($args_str) { start pwsh -Verb RunAs -Arg "-ExecutionPolicy Bypass $args_str" -Wait; exit }
function Start-App($Name) {
    explorer "shell:AppsFolder\$((Get-StartApps $Name | select -f 1).AppID)"
}

function Invoke-Skippable {
    param([string]$Label, [string]$Exe, [string]$Arguments)
    wh "$Label  (Press S to skip)" -Fo Cyan
    $proc = Start-Process $Exe -ArgumentList $Arguments -NoNewWindow -PassThru
    while (!$proc.HasExited) {
        if ([Console]::KeyAvailable) {
            $key = [Console]::ReadKey($true)
            if ($key.Key -eq 'S') {
                taskkill /T /F /PID $proc.Id *>$null
                wh "  Skipped" -Fo Yellow
                return $null
            }
        }
        sleep -Milliseconds 200
    }
    $proc.ExitCode
}

function Copy-Item {
    try {
        Microsoft.PowerShell.Management\Copy-Item @args -ErrorAction Stop
    } catch {
        if ($_.Exception.Message -match "Could not find a part of the path '(.+)'") {
            $parent = Split-Path $Matches[1]
            if ((Read-Host "Create '$parent'? [y/N]") -eq 'y') {
                [void](ni -I Directory $parent -Force)
                Microsoft.PowerShell.Management\Copy-Item @args
            } else { throw }
        } else { throw }
    }
}
