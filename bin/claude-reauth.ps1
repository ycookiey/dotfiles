#!/usr/bin/env pwsh
# claude-reauth — Claude Code OAuth re-auth via ungoogled-chromium
# Usage: claude-reauth <accNum>   (exit 0 = token saved, 1 = failed or cancelled)
param([Parameter(Mandatory)][int]$AccNum)

. "$PSScriptRoot\..\pwsh\aliases.ps1"

$ClientId    = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
$RedirectUri = "https://console.anthropic.com/oauth/code/callback"
$TokenUrl    = "https://console.anthropic.com/v1/oauth/token"
$Scopes      = "org:create_api_key user:profile user:inference"

# Scoop shim overrides --user-data-dir, so use the exe directly
$chromium = "$HOME\scoop\apps\ungoogled-chromium\current\chrome.exe"
if (!(tp $chromium)) { wh "ungoogled-chromium not found (scoop install ungoogled-chromium)" -ForegroundColor Red; exit 1 }

$credDir = "$HOME\.claude-${AccNum}"
if (!(tp $credDir)) { wh "Acc ${AccNum}: config dir not found" -ForegroundColor Red; exit 1 }
$credFile = "$credDir\.credentials.json"

function ConvertTo-Base64Url([byte[]]$b) { [Convert]::ToBase64String($b) -replace "\+","-" -replace "/","_" -replace "=" }

# PKCE. state is a separate random value: it appears in the authorize URL and in the
# code#state string the callback page shows, so it must not be the secret verifier
$verifier  = ConvertTo-Base64Url ([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
$challenge = ConvertTo-Base64Url ([Security.Cryptography.SHA256]::HashData([Text.Encoding]::ASCII.GetBytes($verifier)))
$state     = ConvertTo-Base64Url ([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))

# Open chromium with dedicated profile
$profileDir = "$HOME\.claude-browsers\acc-${AccNum}"
$encRedirect = [uri]::EscapeDataString($RedirectUri)
$encScopes   = [uri]::EscapeDataString($Scopes)
$qs = "code=true&client_id=$ClientId&response_type=code&redirect_uri=$encRedirect&scope=$encScopes&code_challenge=$challenge&code_challenge_method=S256&state=$state"
Start-Process $chromium "--user-data-dir=`"$profileDir`" `"https://claude.ai/oauth/authorize?$qs`""

wh "Acc ${AccNum}: " -ForegroundColor White -NoNewline
wh "chromium opened — authorize and paste the code" -ForegroundColor DarkYellow
$raw = "$(Read-Host "  Code")".Trim()
if (!$raw) { exit 1 }

# Callback returns code#state; a different or missing state means the code is not from this request
$parts = $raw -split '#', 2
if ($parts.Length -lt 2 -or $parts[1] -cne $state) {
    wh "  Acc ${AccNum}: state mismatch - paste the whole code#state string from this login" -ForegroundColor Red
    exit 1
}
$body = @{
    grant_type    = "authorization_code"
    code          = $parts[0]
    state         = $state
    redirect_uri  = $RedirectUri
    client_id     = $ClientId
    code_verifier = $verifier
} | ConvertTo-Json

try {
    $resp = (iwr $TokenUrl -Method Post -Body $body -ContentType "application/json" -TimeoutSec 10 -ea Stop).Content | ConvertFrom-Json
} catch {
    wh "  Acc ${AccNum}: token exchange failed ($($_.Exception.Response.StatusCode))" -ForegroundColor Red
    $err = $_.ErrorDetails.Message
    if ($err) { wh "  $err" -ForegroundColor DarkGray }
    exit 1
}

# Fetch subscription type from profile API
$subType = $null
try {
    $profile = irm "https://api.anthropic.com/api/oauth/profile" -Headers @{
        Authorization = "Bearer $($resp.access_token)"; "anthropic-beta" = "oauth-2025-04-20"
    } -TimeoutSec 5 -ea Stop
    $subType = $profile.account.subscription_type
} catch {}

$cred = if (tp $credFile) { gc $credFile -Raw | ConvertFrom-Json } else { [PSCustomObject]@{} }
$oauth = @{
    accessToken      = $resp.access_token
    refreshToken     = $resp.refresh_token
    expiresAt        = $resp.expires_in ? [DateTimeOffset]::UtcNow.AddSeconds([long]$resp.expires_in).ToUnixTimeMilliseconds() : $null
    scopes           = $Scopes
    subscriptionType = $subType
}
$cred | Add-Member -NotePropertyName claudeAiOauth -NotePropertyValue ([PSCustomObject]$oauth) -Force
$cred | ConvertTo-Json -Depth 10 | Set-Content $credFile -Enc utf8NoBOM
wh "  Acc ${AccNum}: token refreshed" -ForegroundColor Green
exit 0
