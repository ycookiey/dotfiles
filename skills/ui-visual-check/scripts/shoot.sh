#!/usr/bin/env bash
# ui-visual-check: 高DPIスクショ + DOM計測値ダンプ
# 使い方: bash shoot.sh <url> <selector> [output_basename]

set -euo pipefail

if [ $# -lt 2 ]; then
    echo "usage: bash shoot.sh <url> <selector> [output_basename]" >&2
    exit 2
fi

url="$1"
selector="$2"
basename="${3:-$(date +%Y%m%d-%H%M%S)}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out_dir=".agent-output/ui-visual-check"
mkdir -p "$out_dir"

# playwright を scripts 配下にローカルキャッシュ（lock の固定版。未導入・版違いのときだけ npm ci）。
# ESM の bare import "playwright" は祖先の node_modules しか辿れず、
# npx -y の一時 node_modules では解決できない。scripts/node_modules に固定する。
# node_modules/ は .gitignore 済み。
# sed は最初の一致で終了する（head で切ると pipefail 下で SIGPIPE になりうる）
want_ver="$(sed -n '/"playwright": *"/{s/.*"playwright": *"\([^"]*\)".*/\1/p;q}' "$script_dir/package.json" | tr -d '\r')"
have_ver=""
if [ -f "$script_dir/node_modules/playwright/package.json" ]; then
    have_ver="$(sed -n '/^ *"version": *"/{s/^ *"version": *"\([^"]*\)".*/\1/p;q}' "$script_dir/node_modules/playwright/package.json" | tr -d '\r')"
fi
if [ -z "$have_ver" ] || [ "$have_ver" != "$want_ver" ]; then
    echo "ui-visual-check: installing playwright $want_ver (npm ci)..." >&2
    (cd "$script_dir" && npm ci --no-fund --no-audit >&2)
fi
# Chromium バイナリ取得は、この版の executablePath が無いときだけ（npx の起動を省く）。
# install chromium は headless shell も入れる。判定に失敗したときも install に倒す。
if ! (cd "$script_dir" && node -e "process.exit(require('fs').existsSync(require('playwright').chromium.executablePath())?0:1)" 2>/dev/null); then
    (cd "$script_dir" && npx playwright install chromium >&2)
fi

node "$script_dir/shoot.mjs" "$url" "$selector" "$out_dir/$basename"
