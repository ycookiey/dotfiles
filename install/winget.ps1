$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
$WingetFile = "$ScriptDir\wingetfile.json"

if (!(gcm winget -ea 0)) {
    wh "winget not found. Installing..." -Fo Yellow
    $tmp = "$env:TEMP\winget-install"
    [void](ni $tmp -ItemType Directory -Force)
    $ProgressPreference = 'SilentlyContinue'

    # 依存: VCLibs
    $vclibs = "$tmp\vclibs.appx"
    if (!(Get-AppxPackage Microsoft.VCLibs.140.00.UWPDesktop -ea 0)) {
        Invoke-WebRequest -Uri "https://aka.ms/Microsoft.VCLibs.x64.14.00.Desktop.appx" -OutFile $vclibs -UseBasicParsing
        Add-AppxPackage $vclibs
    }

    # 依存: UI.Xaml
    if (!(Get-AppxPackage Microsoft.UI.Xaml.2.8 -ea 0)) {
        $xamlNupkg = "$tmp\xaml.zip"
        Invoke-WebRequest -Uri "https://www.nuget.org/api/v2/package/Microsoft.UI.Xaml/2.8.6" -OutFile $xamlNupkg -UseBasicParsing
        Expand-Archive $xamlNupkg "$tmp\xaml" -Force
        Add-AppxPackage "$tmp\xaml\tools\AppX\x64\Release\Microsoft.UI.Xaml.2.8.appx"
    }

    # winget 本体
    $release = Invoke-RestMethod "https://api.github.com/repos/microsoft/winget-cli/releases/latest"
    $msix = $release.assets | Where-Object { $_.name -match '\.msixbundle$' } | Select-Object -First 1
    $license = $release.assets | Where-Object { $_.name -match 'License.*\.xml$' } | Select-Object -First 1
    $msixPath = "$tmp\winget.msixbundle"
    Invoke-WebRequest -Uri $msix.browser_download_url -OutFile $msixPath -UseBasicParsing

    if ($license) {
        $licPath = "$tmp\license.xml"
        Invoke-WebRequest -Uri $license.browser_download_url -OutFile $licPath -UseBasicParsing
        Add-AppxProvisionedPackage -Online -PackagePath $msixPath -LicensePath $licPath -ErrorAction SilentlyContinue
    }
    Add-AppxPackage $msixPath

    Remove-Item $tmp -Recurse -Force -ea 0
    $ProgressPreference = 'Continue'

    if (!(gcm winget -ea 0)) {
        wh "winget installation failed. Skipping." -Fo Yellow
        return
    }
    wh "winget installed." -Fo Green
}

$json = gc $WingetFile -Raw | ConvertFrom-Json

# Cache winget list output for installed check
$wingetOutput = (winget list 2>$null) -join "`n"

foreach ($app in $json.apps) {
    if ($wingetOutput -match ([regex]::Escape($app.Id) + '\s')) {
        continue
    }
    wh "Installing $($app.Id)..." -Fo Cyan
    $overrides = @{
        'Microsoft.VisualStudio.BuildTools' = '--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --quiet --wait'
    }
    $wingetArgs = @('install', '--id', $app.Id, '-e', '--accept-source-agreements', '--accept-package-agreements')
    if ($overrides[$app.Id]) { $wingetArgs += '--override', $overrides[$app.Id] }
    winget @wingetArgs
}

wh "`nWinget setup complete." -Fo Green
