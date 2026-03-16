$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
$WingetFile = "$ScriptDir\wingetfile.json"

if (!(gcm winget -ea 0)) {
    wh "winget not found. Skipping winget setup." -Fg Yellow
    return
}

$json = gc $WingetFile -Raw | ConvertFrom-Json

# Cache list of installed apps once to avoid repeated winget invocations in the loop
$installedAppIds = @{}
$installedAppsOutput = winget list --output json 2>$null
if ($LASTEXITCODE -eq 0 -and $installedAppsOutput) {
    try {
        # winget output is captured as an array of lines; convert to a single JSON string first
        $installedAppsJson = $installedAppsOutput | Out-String
        $installedApps = $installedAppsJson | ConvertFrom-Json
        foreach ($pkg in $installedApps) {
            # Support both possible property names depending on winget version
            $pkgId = $null
            if ($pkg.PSObject.Properties.Name -contains 'Id') {
                $pkgId = $pkg.Id
            } elseif ($pkg.PSObject.Properties.Name -contains 'PackageIdentifier') {
                $pkgId = $pkg.PackageIdentifier
            }
            if ($pkgId) {
                $installedAppIds[$pkgId] = $true
            }
        }
    } catch {
        # If parsing fails, leave $installedAppIds empty and let winget handle re-installs
    }
}

foreach ($app in $json.apps) {
    if ($installedAppIds.ContainsKey($app.Id)) {
        continue
    }
    wh "Installing $($app.Id)..." -Fg Cyan
    winget install --id $app.Id -e --accept-source-agreements --accept-package-agreements
}

wh "`nWinget setup complete." -Fg Green
