#Requires -Version 7.0
# git-new [name] [-Here] — 新規Gitリポジトリを作成してGitHubにプッシュ
param(
    [Parameter(Position = 0)]
    [string]$Name,
    [Alias('h')]
    [switch]$Here
)

$ErrorActionPreference = 'Stop'
# EAP=Stop does not cover native exit codes before PS 7.4, so each git step is checked
# explicitly: a failed init/add/commit must never reach gh repo create --push.
function Assert-NativeOk([string]$Step) {
    if ($LASTEXITCODE) { throw "$Step failed (exit $LASTEXITCODE); GitHub repo not created" }
}
. "$PSScriptRoot\..\pwsh\aliases.ps1"
$DefaultRoot = 'C:\Main\Project'

if (!$Name) {
    $Name = Split-Path -Leaf (gl)
} else {
    $BaseDir = $Here ? (gl) : $DefaultRoot
    $TargetPath = "$BaseDir\$Name"
    if (!(tp $TargetPath)) { [void](ni -I Directory $TargetPath) }
    cd $TargetPath
}

git init -b main
Assert-NativeOk 'git init'
sc 'README.md' "# $Name" -No

@'
# OS
.DS_Store
Thumbs.db
Desktop.ini
nul

# Editors/IDEs
.idea/
.history/
*.swp
*.swo

# Logs
*.log
'@ | sc '.gitignore'

git add README.md .gitignore
Assert-NativeOk 'git add'
git commit -m 'initial commit'
Assert-NativeOk 'git commit'
gh repo create $Name --private --source=. --remote=origin --push
