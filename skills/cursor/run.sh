#!/usr/bin/env bash
# cursor agent exec wrapper — プロンプトをファイル経由で引数指示
# Usage: run.sh <dir> <request> [plan_file] [workspace]
set -euo pipefail

dir="${1:-.}"
request="$2"
plan_file="${3:-}"
workspace="${4:-$dir}"

summary_file="$dir/.cursor-summary.md"
prompt_file="$dir/.cursor-prompt.md"
log_file="$dir/.cursor-agent.log"
rm -f "$summary_file"

# プロンプトをファイルに書き出す
{
  if [[ -n "$plan_file" && -f "$plan_file" ]]; then
    printf '## 計画\n\n'
    cat "$plan_file"
    printf '\n\n---\n\n'
  fi
  printf '%s' "$request"
  printf '\n\n## 完了後の作業\n%s/.cursor-summary.md に変更したファイル一覧と各変更の要点（1行ずつ）を記載せよ。\n' "$dir"
} > "$prompt_file"

SECONDS=0
agent --print --yolo --trust --workspace "$workspace" \
  "$prompt_file を読み、書かれた指示をすべて実行せよ。確認や質問は不要。" \
  > "$log_file" 2>&1 || true
elapsed=$SECONDS

rm -f "$prompt_file"

if [[ -f "$summary_file" ]]; then
  printf '[%dm%ds]\n' $((elapsed/60)) $((elapsed%60))
  cat "$summary_file"
else
  echo "[warn] .cursor-summary.md が生成されなかった。"
  echo "--- git diff --stat ---"
  git -C "$dir" diff --stat 2>/dev/null || true
  echo "--- agent log (last 20 lines) ---"
  tail -20 "$log_file" 2>/dev/null || true
fi
