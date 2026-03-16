$ErrorActionPreference = 'Stop'

# Claude Code — npm 経由でインストール
if (gcm claude -ea 0) {
    wh "Claude Code is already installed." -Fg Green
    return
}

if (!(gcm npm -ea 0)) {
    wh "npm not found. Install Node.js first (mise install node)." -Fg Yellow
    return
}

# npm 側でもインストール済みか確認
$global = npm list -g @anthropic-ai/claude-code 2>$null
if ($global -match 'claude-code') {
    wh "Claude Code is already installed." -Fg Green
    return
}

wh "Installing Claude Code..." -Fg Cyan
npm install -g @anthropic-ai/claude-code
wh "Claude Code installed." -Fg Green
