#!/usr/bin/env bash
# gcc (GLM Claude Code) — Agent SDK wrapper
# Usage: run.sh <dir> <request> [plan_file] [--resume <sessionId>]
set -euo pipefail

dir="${1:-.}"
request="${2:-}"
plan_file="${3:-}"
resume_flag=""
# --resume が第4引数以降にある場合
shift 3 2>/dev/null || true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --resume) resume_flag="--resume $2"; shift 2 ;;
    *) shift ;;
  esac
done

script_dir="$(cd "$(dirname "$0")" && pwd)"
prompt_file="$dir/.gcc-prompt.md"

# プロンプト組み立て
{
  if [[ -n "$plan_file" && -f "$plan_file" ]]; then
    printf '## 計画\n\n'
    cat "$plan_file"
    printf '\n\n---\n\n'
  fi
  if [[ -n "$request" ]]; then
    printf '%s' "$request"
  fi
  printf '\n\n## 完了後の作業\n%s/.gcc-summary.md に変更したファイル一覧と各変更の要点（1行ずつ）を記載せよ。\n' "$dir"
} > "$prompt_file"

SECONDS=0
result_file="$dir/.gcc-result.json"
# shellcheck disable=SC2086
node "$script_dir/gcc.mjs" "$dir" "$prompt_file" $resume_flag > "$result_file" 2>/dev/null || true
elapsed=$SECONDS

rm -f "$prompt_file"

printf '[%dm%ds]\n' $((elapsed/60)) $((elapsed%60))

# JSON結果をパース（/dev/stdin は Windows Node.js 非対応のためファイル経由）
if node -e "
  const r = JSON.parse(require('fs').readFileSync(process.argv[1], 'utf8'));
  if (r.sessionId) console.log('sessionId: ' + r.sessionId);
  if (r.error) { console.error('[error] ' + r.error); process.exit(1); }
  if (r.summary) { console.log(r.summary); process.exit(0); }
  process.exit(1);
" "$result_file" 2>/dev/null; then
  :
else
  echo "[warn] .gcc-summary.md が生成されなかった。"
  echo "--- git diff --stat ---"
  git -C "$dir" diff --stat 2>/dev/null || true
  echo "--- agent log (last 20 lines) ---"
  tail -20 "$dir/.gcc-agent.log" 2>/dev/null || true
fi

rm -f "$result_file"
