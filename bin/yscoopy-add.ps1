param(
  [Parameter(Mandatory)][string]$App,
  [Parameter(Mandatory)][string]$Repo,
  [Parameter(Mandatory)][string]$Description
)

$cfgPath = "$HOME/.config/yscoopy.json"
$owner = 'ycookiey'
$bucket = 'yscoopy'

# --- Config ---
if (!(tp $cfgPath)) {
  Write-Error "Config not found: $cfgPath`nCreate it with: { `"worker_url`": `"..`", `"webhook_secret`": `"..`" }"
  return
}
$cfg = gc $cfgPath -Raw | ConvertFrom-Json
if (!$cfg.worker_url -or !$cfg.webhook_secret) {
  Write-Error "Config must have worker_url and webhook_secret"
  return
}

# --- Latest release ---
wh "Fetching latest release from $owner/$Repo..." -Fg Cyan
$release = gh api "repos/$owner/$Repo/releases/latest" --jq '{tag: .tag_name, assets: [.assets[] | {name: .name, url: .browser_download_url}]}' | ConvertFrom-Json
if (!$release) { Write-Error "No release found for $owner/$Repo"; return }

$version = $release.tag -replace '^v', ''
# Find Windows asset: bare .exe or windows .zip
$asset = $release.assets | ? { $_.name -match '\.exe$' } | select -First 1
$isZip = $false
if (!$asset) {
  $asset = $release.assets | ? { $_.name -match 'windows.*\.zip$' } | select -First 1
  $isZip = $true
}
if (!$asset) { Write-Error "No Windows asset (.exe or .zip) found in release"; return }

# --- Hash ---
wh "Downloading $($asset.name) for hash..." -Fg Cyan
$tmp = "$env:TEMP/$($asset.name)"
iwr $asset.url -OutFile $tmp
$hash = (Get-FileHash $tmp -Algorithm SHA256).Hash.ToLower()

if ($isZip) {
  $zip = [IO.Compression.ZipFile]::OpenRead($tmp)
  $exeName = ($zip.Entries | ? { $_.Name -match '\.exe$' } | select -First 1).Name
  $zip.Dispose()
} else {
  $exeName = $asset.name
}
rm $tmp
if (!$exeName) { Write-Error "No .exe found in zip"; return }

# --- Manifest ---
$manifest = [ordered]@{
  version     = $version
  description = $Description
  homepage    = "https://github.com/$owner/$Repo"
  license     = 'MIT'
  url         = "https://github.com/$owner/$Repo/releases/download/v$version/$($asset.name)"
  hash        = $hash
  bin         = $exeName
  shortcuts   = @(, @($exeName, $App))
  checkver    = @{ github = "https://github.com/$owner/$Repo" }
  autoupdate  = @{ url = "https://github.com/$owner/$Repo/releases/download/v`$version/$($asset.name)" }
}
if ($isZip) { $manifest['extract_dir'] = $exeName -replace '\.exe$', '' }
$json = $manifest | ConvertTo-Json -Depth 4

wh "`nManifest:" -Fg Green
wh $json

# --- Push to yscoopy ---
wh "`nPushing manifest to $bucket..." -Fg Cyan
$encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
$existing = gh api "repos/$owner/$bucket/contents/$App.json" --jq '.sha' 2>$null

$body = @{ message = "add $App"; content = $encoded }
if ($existing) { $body.sha = $existing }
$bodyJson = $body | ConvertTo-Json -Compress

$bodyJson | gh api "repos/$owner/$bucket/contents/$App.json" -X PUT --input - > $null
if ($LASTEXITCODE -ne 0) { Write-Error "Failed to push manifest"; return }
wh "Manifest pushed." -Fg Green

# --- Webhook ---
wh "Registering webhook on $owner/$Repo..." -Fg Cyan
$hooks = gh api "repos/$owner/$Repo/hooks" --jq '.[].config.url' 2>$null
if ($hooks -and ($hooks -split "`n") -contains $cfg.worker_url) {
  wh "Webhook already registered." -Fg Yellow
} else {
  $hookBody = @{
    config = @{
      url          = $cfg.worker_url
      content_type = 'json'
      secret       = $cfg.webhook_secret
    }
    events = @('release')
    active = $true
  } | ConvertTo-Json -Depth 3 -Compress

  $hookBody | gh api "repos/$owner/$Repo/hooks" -X POST --input - > $null
  if ($LASTEXITCODE -ne 0) { Write-Error "Failed to register webhook"; return }
  wh "Webhook registered." -Fg Green
}

wh "`nDone! $App added to $bucket." -Fg Green
