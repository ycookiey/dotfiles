#!/usr/bin/env pwsh
# claude-reauth — Claude Code OAuth re-auth via ungoogled-chromium
# Usage: claude-reauth <accNum>
param([Parameter(Mandatory)][int]$AccNum)

. "$PSScriptRoot\..\pwsh\aliases.ps1"
Add-Type -AN System.Security.Cryptography

$ClientId    = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
$RedirectUri = "https://console.anthropic.com/oauth/code/callback"
$TokenUrl    = "https://console.anthropic.com/v1/oauth/token"
$Scopes      = "org:create_api_key user:profile user:inference"

# Scoop shim overrides --user-data-dir, so use the exe directly
$chromium = "$HOME\scoop\apps\ungoogled-chromium\current\chrome.exe"
if (!(tp $chromium)) { wh "ungoogled-chromium not found (scoop install ungoogled-chromium)" -ForegroundColor Red; return $false }

$credDir = "$HOME\.claude-${AccNum}"
if (!(tp $credDir)) { wh "Acc ${AccNum}: config dir not found" -ForegroundColor Red; return $false }
$credFile = "$credDir\.credentials.json"

# PKCE
$bytes = [byte[]]::new(32)
[Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
$verifier = [Convert]::ToBase64String($bytes) -replace "\+","-" -replace "/","_" -replace "="
$hash = [Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::ASCII.GetBytes($verifier))
$challenge = [Convert]::ToBase64String($hash) -replace "\+","-" -replace "/","_" -replace "="

# Open chromium with dedicated profile
$profileDir = "$HOME\.claude-browsers\acc-${AccNum}"
$encRedirect = [uri]::EscapeDataString($RedirectUri)
$encScopes   = [uri]::EscapeDataString($Scopes)
$qs = "code=true&client_id=$ClientId&response_type=code&redirect_uri=$encRedirect&scope=$encScopes&code_challenge=$challenge&code_challenge_method=S256&state=$verifier"
Start-Process $chromium "--user-data-dir=`"$profileDir`" `"https://claude.ai/oauth/authorize?$qs`""

wh "Acc ${AccNum}: " -ForegroundColor White -NoNewline
wh "chromium opened — authorize and paste the code" -ForegroundColor DarkYellow
$raw = Read-Host "  Code"
if (!$raw) { return $false }

# Callback returns code#state
$parts = $raw -split '#', 2
$body = @{
    grant_type    = "authorization_code"
    code          = $parts[0]
    state         = $parts.Length -gt 1 ? $parts[1] : $null
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
    return $false
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
    scopes           = "org:create_api_key user:profile user:inference"
    subscriptionType = $subType
}
$cred | Add-Member -NotePropertyName claudeAiOauth -NotePropertyValue ([PSCustomObject]$oauth) -Force
$cred | ConvertTo-Json -Depth 10 | Set-Content $credFile -Enc utf8NoBOM
wh "  Acc ${AccNum}: token refreshed" -ForegroundColor Green
return $true
