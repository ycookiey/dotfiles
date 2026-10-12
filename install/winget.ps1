$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"
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

    # winget を入れられない環境 (LTSC / Sandbox 等) では wingetfile.json のアプリが 1 つも入らないので、
    # 失敗として exit 1 で返す。setup.ps1 は 'Winget apps' 手順の失敗に数え、bootstrap は最後の一覧に載せる
    # (他の手順・Phase は止まらない)
    if (!(gcm winget -ea 0)) {
        wh "winget installation failed: no winget apps were installed. Install App Installer (winget), then rerun install\winget.ps1" -Fo Red
        exit 1
    }
    wh "winget installed." -Fo Green
}

$json = gc $WingetFile -Raw | ConvertFrom-Json

# Cache winget list output for installed check
# --accept-source-agreements/--disable-interactivity: 同意前の新 PC で Y/N プロンプトが捕捉された出力に隠れて止まるのを防ぐ
$wingetOutput = (winget list --accept-source-agreements --disable-interactivity 2>$null) -join "`n"

$overrides = @{
    'Microsoft.VisualStudio.BuildTools' = '--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --quiet --wait'
}

# winget の非 0 のうち失敗に数えないもの (値は Process.ExitCode と同じ符号付き 32bit)
# https://github.com/microsoft/winget-cli/blob/master/doc/windows/package-manager/winget/returnCodes.md
# 上の導入済み判定は winget list の表示文字列への一致だけなので、漏れた ID は install が走り
# 「既に入っている」系の非 0 で終わる。パッケージは入っているので成功として扱う
$alreadyInstalledCodes = @(
    -1978335189  # 0x8A15002B UPDATE_NOT_APPLICABLE
    -1978335135  # 0x8A150061 PACKAGE_ALREADY_INSTALLED
    -1978335153  # 0x8A15004F UPGRADE_VERSION_NOT_NEWER
    -1978335128  # 0x8A150068 PACKAGE_IS_PINNED
    -1978334963  # 0x8A15010D INSTALL_ALREADY_INSTALLED
    -1978334962  # 0x8A15010E INSTALL_DOWNGRADE
    -1978334956  # 0x8A150114 INSTALL_UPGRADE_NOT_SUPPORTED
)
# 再起動で導入が終わるもの (VS BuildTools 等)。失敗に数えず、最後に再起動を促す
# (INSTALL_REBOOT_REQUIRED_FOR_INSTALL 0x8A15010A は導入できていないので失敗のまま)
$rebootToFinishCodes = @(
    -1978334967  # 0x8A150109 INSTALL_REBOOT_REQUIRED_TO_FINISH
    -1978334965  # 0x8A15010B INSTALL_REBOOT_INITIATED
)

# 失敗 (上以外の非 0) は集めて最後に一覧を出す。S キーのスキップ ($null) は失敗に数えない
$failed = @()
$needsReboot = @()
foreach ($app in $json.apps) {
    if ($wingetOutput -match ([regex]::Escape($app.Id) + '\s')) {
        continue
    }
    $argStr = "install --id $($app.Id) -e --accept-source-agreements --accept-package-agreements"
    if ($overrides[$app.Id]) { $argStr += " --override `"$($overrides[$app.Id])`"" }
    $rc = Invoke-Skippable "Installing $($app.Id)" winget $argStr
    if ($null -eq $rc -or $rc -eq 0) { continue }
    if ($rc -in $alreadyInstalledCodes) {
        wh "  Already installed: $($app.Id) (exit $rc)" -Fo DarkGray
    } elseif ($rc -in $rebootToFinishCodes) {
        wh "  Restart needed to finish: $($app.Id) (exit $rc)" -Fo Yellow
        $needsReboot += $app.Id
    } else {
        wh "  Failed: $($app.Id) (exit $rc)" -Fo Yellow
        $failed += $app.Id
    }
}

if ($needsReboot.Count -gt 0) {
    wh "`nRestart your PC to finish installing: $($needsReboot -join ', ')" -Fo Yellow
}

if ($failed.Count -gt 0) {
    wh "`nFailed winget apps: $($failed -join ', ')" -Fo Yellow
    wh "Retry:" -Fo Yellow
    foreach ($id in $failed) { wh "  winget install --id $id -e" -Fo Yellow }
    exit 1
}

wh "`nWinget setup complete." -Fo Green
# 途中の native コマンド (winget list) の $LASTEXITCODE を呼び出し元に残さない
exit 0
