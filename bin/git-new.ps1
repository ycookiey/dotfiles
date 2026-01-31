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
    [string]$Name
)

$ErrorActionPreference = 'Stop'

# repo-name省略時はカレントフォルダ名を使う
if (-not $Name) {
    $Name = Split-Path -Leaf (Get-Location)
} else {
    # repo-name指定ならフォルダも作る
    if (-not (Test-Path $Name)) {
        New-Item -ItemType Directory -Path $Name | Out-Null
    }
    Set-Location $Name
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
