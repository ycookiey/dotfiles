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

# playwright を scripts 配下にローカルキャッシュ（初回のみ）。
# ESM の bare import "playwright" は祖先の node_modules しか辿れず、
# npx -y の一時 node_modules では解決できない。scripts/node_modules に固定する。
# node_modules/ は .gitignore 済み。
if [ ! -d "$script_dir/node_modules/playwright" ]; then
    echo "ui-visual-check: installing playwright (first run only)..." >&2
    (cd "$script_dir" && npm install --no-save --no-fund --no-audit playwright@latest >&2)
fi
# Chromium バイナリ取得（未取得ならDL、取得済みなら即終了。ローカル playwright を使う）
(cd "$script_dir" && npx playwright install chromium >&2)

node "$script_dir/shoot.mjs" "$url" "$selector" "$out_dir/$basename"
