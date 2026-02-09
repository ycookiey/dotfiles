#Requires -Version 5.1
<#
.SYNOPSIS
    新規Gitリポジトリを作成してGitHubにプッシュする
.EXAMPLE
    git-new           # カレントフォルダ名でリポジトリ作成
    git-new my-repo   # my-repoフォルダを作成してリポジトリ作成
#>
param(
    [Parameter(Position = 0)]
    [string]$Name,
    # カレントディレクトリに作成する（デフォルトは C:\Main\Project）
    [Alias('h')]
    [switch]$Here
)

$ErrorActionPreference = 'Stop'
$DefaultRoot = 'C:\Main\Project'

if (-not $Name) {
    # 名前省略: カレントフォルダをそのまま使う
    $Name = Split-Path -Leaf (Get-Location)
} else {
    $BaseDir = if ($Here) { Get-Location } else { $DefaultRoot }
    $TargetPath = Join-Path $BaseDir $Name
    if (-not (Test-Path $TargetPath)) {
        New-Item -ItemType Directory -Path $TargetPath | Out-Null
    }
    Set-Location $TargetPath
}

git init -b main

# README
Set-Content -Path 'README.md' -Value "# $Name" -NoNewline

# .gitignore
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
'@ | Set-Content -Path '.gitignore'

git add -A
git commit -m 'chore: initial commit'

# GitHubにPrivateで作ってpush
gh repo create $Name --private --source=. --remote=origin --push
