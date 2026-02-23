#Requires -Version 5.1
# git-new [name] [-Here] — 新規Gitリポジトリを作成してGitHubにプッシュ
param(
    [Parameter(Position = 0)]
    [string]$Name,
    [Alias('h')]
    [switch]$Here
)

$ErrorActionPreference = 'Stop'
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
git commit -m 'initial commit'
gh repo create $Name --private --source=. --remote=origin --push
