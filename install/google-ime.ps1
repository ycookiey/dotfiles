$ErrorActionPreference = 'Stop'

# Google 日本語入力 — winget 経由でインストール
if (!(gcm winget -ea 0)) {
    wh "winget not found. Skipping Google Japanese Input." -Fg Yellow
    return
}

$list = winget list --id Google.JapaneseInput 2>$null
if ($LASTEXITCODE -eq 0 -and $list -match 'Google.JapaneseInput') {
    wh "Google Japanese Input is already installed." -Fg Green
    return
}

wh "Installing Google Japanese Input..." -Fg Cyan
winget install --id Google.JapaneseInput -e --accept-source-agreements --accept-package-agreements
wh "Google Japanese Input installed." -Fg Green
