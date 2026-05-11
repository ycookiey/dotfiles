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

# Playwright は常駐インストール不要、npx -y で都度起動
# Chromium が未取得なら自動DLされるので初回のみ時間がかかる
npx -y -p playwright@latest node "$script_dir/shoot.mjs" "$url" "$selector" "$out_dir/$basename"
