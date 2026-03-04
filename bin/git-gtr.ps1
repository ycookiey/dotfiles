#!/usr/bin/env pwsh
# git-gtr PowerShell wrapper
# Allows running git-gtr from PowerShell by invoking bash

# Get the directory where this script is located
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# Path to the main gtr script
$GtrScript = Join-Path $ScriptDir "gtr"

# Find bash executable (try common locations)
$BashPath = $null
$BashLocations = @(
    "C:\Program Files\Git\bin\bash.exe",
    "C:\Program Files (x86)\Git\bin\bash.exe",
    "$env:ProgramFiles\Git\bin\bash.exe",
    "$env:ProgramFiles(x86)\Git\bin\bash.exe"
)

foreach ($location in $BashLocations) {
    if (Test-Path $location) {
        $BashPath = $location
        break
    }
}

# If not found in common locations, try PATH
if (-not $BashPath) {
    $BashPath = (Get-Command bash -ErrorAction SilentlyContinue).Source
}

if (-not $BashPath) {
    Write-Error "Git Bash not found. Please install Git for Windows from https://git-scm.com/"
    exit 1
}

# Convert Windows path to Git Bash path format (C:\path -> /c/path)
$GtrScriptUnix = $GtrScript -replace '\\', '/' -replace '^([A-Z]):', '/$1' -replace '^/([A-Z])/', { "/$($_.Groups[1].Value.ToLower())/" }

# Execute the bash script with all arguments
& $BashPath -c "$GtrScriptUnix $args"

# Return the exit code from bash
exit $LASTEXITCODE
