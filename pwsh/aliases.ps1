# user コマンド追加は cli/definitions.toml (dotcli generate)。ここは install/setup 用内部 alias。

$alias:tp = 'Test-Path'
$alias:sc = 'Set-Content'
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

# 標準 Copy-Item の proxy (cp alias もここに解決される)。対話 session に限り、-Destination の親 dir が
# 無ければ作成を確認する。pipeline 入力・common parameter・エラーの種類は標準 cmdlet のまま。
# param/dynamicparam/begin/process/end は ProxyCommand::Create で生成したもの (pwsh 7.5)。
# 生成物にある clean block は Windows PowerShell 5.1 で parse error になり本 file 全体が読めなくなるため省く。
function Copy-Item {
    [CmdletBinding(DefaultParameterSetName='Path', SupportsShouldProcess=$true, ConfirmImpact='Medium', HelpUri='https://go.microsoft.com/fwlink/?LinkID=2096990')]
    param(
        [Parameter(ParameterSetName='Path', Mandatory=$true, Position=0, ValueFromPipeline=$true, ValueFromPipelineByPropertyName=$true)]
        [string[]]
        ${Path},

        [Parameter(ParameterSetName='LiteralPath', Mandatory=$true, ValueFromPipelineByPropertyName=$true)]
        [Alias('PSPath','LP')]
        [string[]]
        ${LiteralPath},

        [Parameter(Position=1, ValueFromPipelineByPropertyName=$true)]
        [string]
        ${Destination},

        [switch]
        ${Container},

        [switch]
        ${Force},

        [string]
        ${Filter},

        [string[]]
        ${Include},

        [string[]]
        ${Exclude},

        [switch]
        ${Recurse},

        [switch]
        ${PassThru},

        [Parameter(ValueFromPipelineByPropertyName=$true)]
        [pscredential]
        [System.Management.Automation.CredentialAttribute()]
        ${Credential})

    dynamicparam
    {
        try {
            $targetCmd = $ExecutionContext.InvokeCommand.GetCommand('Microsoft.PowerShell.Management\Copy-Item', [System.Management.Automation.CommandTypes]::Cmdlet, $PSBoundParameters)
            $dynamicParams = @($targetCmd.Parameters.GetEnumerator() | Microsoft.PowerShell.Core\Where-Object { $_.Value.IsDynamic })
            if ($dynamicParams.Length -gt 0)
            {
                $paramDictionary = [Management.Automation.RuntimeDefinedParameterDictionary]::new()
                foreach ($param in $dynamicParams)
                {
                    $param = $param.Value

                    if(-not $MyInvocation.MyCommand.Parameters.ContainsKey($param.Name))
                    {
                        $dynParam = [Management.Automation.RuntimeDefinedParameter]::new($param.Name, $param.ParameterType, $param.Attributes)
                        $paramDictionary.Add($param.Name, $dynParam)
                    }
                }

                return $paramDictionary
            }
        } catch {
            throw
        }
    }

    begin
    {
        # 親 dir の事前確認。pipeline で property から束縛される -Destination は begin 時点で未束縛なので対象外。
        # 確認しない/作らない場合は何もせず、標準 cmdlet 自身の DirectoryNotFound 系エラーに任せる。
        if ($PSBoundParameters.ContainsKey('Destination') -and -not $PSBoundParameters.ContainsKey('ToSession') -and -not $WhatIfPreference) {
            $parent = $null
            try {
                $provider = $null; $drive = $null
                $full = $PSCmdlet.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Destination, [ref]$provider, [ref]$drive)
                if ($provider.Name -eq 'FileSystem' -and -not [IO.Directory]::Exists($full) -and -not [IO.File]::Exists($full)) {
                    $p = [IO.Path]::GetDirectoryName($full.TrimEnd('\', '/'))
                    if ($p -and -not [IO.Directory]::Exists($p)) { $parent = $p }
                }
            } catch {}

            if ($parent -and -not [Console]::IsInputRedirected) {
                # pwsh 起動引数の -NonInteractive (省略形 -noni 以上、-/--// 前置) を探す。
                # -Command/-File/-EncodedCommand/-CommandWithArgs 以降は script 側の引数なので見ない。
                $nonInteractive = $false
                foreach ($a in [Environment]::GetCommandLineArgs() | Microsoft.PowerShell.Utility\Select-Object -Skip 1) {
                    if ($a -notmatch '^(?:--?|/)(\w+)$') { continue }
                    $k = $Matches[1]
                    if ($k.Length -ge 4 -and 'noninteractive'.StartsWith($k, [StringComparison]::OrdinalIgnoreCase)) { $nonInteractive = $true; break }
                    if ($k -in 'ec', 'cwa' -or @('command', 'file', 'encodedcommand', 'commandwithargs') -match "^$([regex]::Escape($k))") { break }
                }
                if (-not $nonInteractive) {
                    $answer = $null
                    # NonInteractive の検出漏れ時は Read-Host が PSInvalidOperationException を投げる。その場合も確認なしで続行
                    try { $answer = Microsoft.PowerShell.Utility\Read-Host "Create '$parent'? [y/N]" } catch [System.Management.Automation.PSInvalidOperationException] {}
                    if ($answer -eq 'y') { [void][IO.Directory]::CreateDirectory($parent) }
                }
            }
        }

        try {
            $outBuffer = $null
            if ($PSBoundParameters.TryGetValue('OutBuffer', [ref]$outBuffer))
            {
                $PSBoundParameters['OutBuffer'] = 1
            }

            $wrappedCmd = $ExecutionContext.InvokeCommand.GetCommand('Microsoft.PowerShell.Management\Copy-Item', [System.Management.Automation.CommandTypes]::Cmdlet)
            $scriptCmd = {& $wrappedCmd @PSBoundParameters }

            $steppablePipeline = $scriptCmd.GetSteppablePipeline($myInvocation.CommandOrigin)
            $steppablePipeline.Begin($PSCmdlet)
        } catch {
            throw
        }
    }

    process
    {
        try {
            $steppablePipeline.Process($_)
        } catch {
            throw
        }
    }

    end
    {
        try {
            $steppablePipeline.End()
        } catch {
            throw
        }
    }
    <#

    .ForwardHelpTargetName Microsoft.PowerShell.Management\Copy-Item
    .ForwardHelpCategory Cmdlet

    #>
}
