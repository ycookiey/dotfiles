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
result_file="$dir/.gcc-result.json"
stderr_file="$dir/.gcc-stderr.log"
summary_file="$dir/.gcc-summary.md"
log_file="$dir/.gcc-agent.log"
# 中断時も一時ファイルを作業 dir に残さない
trap 'rm -f "$prompt_file" "$result_file"' EXIT
# 前回の summary/ログを今回の結果と取り違えない
rm -f "$summary_file" "$log_file" "$stderr_file"

# 依存を lock どおりに導入（未導入、または package.json の固定版と異なるとき）
sdk_pkg="$script_dir/node_modules/@anthropic-ai/claude-agent-sdk/package.json"
# sed は最初の一致で終了する（head で切ると pipefail 下で SIGPIPE になりうる）
want_ver="$(sed -n '/"@anthropic-ai\/claude-agent-sdk": *"/{s/.*"@anthropic-ai\/claude-agent-sdk": *"\([^"]*\)".*/\1/p;q}' "$script_dir/package.json" | tr -d '\r')"
have_ver=""
if [[ -f "$sdk_pkg" ]]; then
  have_ver="$(sed -n '/^ *"version": *"/{s/^ *"version": *"\([^"]*\)".*/\1/p;q}' "$sdk_pkg" | tr -d '\r')"
fi
if [[ -z "$have_ver" || "$have_ver" != "$want_ver" ]]; then
  echo "gcc: installing dependencies (npm ci)..." >&2
  (cd "$script_dir" && npm ci --no-fund --no-audit >&2)
fi

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
# shellcheck disable=SC2086
node "$script_dir/gcc.mjs" "$dir" "$prompt_file" $resume_flag > "$result_file" 2> "$stderr_file" || true
elapsed=$SECONDS

printf '[%dm%ds]\n' $((elapsed/60)) $((elapsed%60))

# JSON結果をパース（/dev/stdin は Windows Node.js 非対応のためファイル経由）
# 空や壊れた JSON は黙って失敗扱いにし、r.error は表示する
if node -e "
  let r;
  try { r = JSON.parse(require('fs').readFileSync(process.argv[1], 'utf8')); } catch { process.exit(1); }
  if (r.sessionId) console.log('sessionId: ' + r.sessionId);
  if (r.error) { console.error('[error] ' + r.error); process.exit(1); }
  if (r.summary) { console.log(r.summary); process.exit(0); }
  process.exit(1);
" "$result_file"; then
  :
else
  echo "[warn] .gcc-summary.md が生成されなかった。"
  echo "--- git diff --stat ---"
  git -C "$dir" diff --stat 2>/dev/null || true
  echo "--- agent log (last 20 lines) ---"
  tail -20 "$log_file" 2>/dev/null || true
  if [[ -s "$stderr_file" ]]; then
    echo "--- node stderr (last 20 lines) ---"
    tail -20 "$stderr_file"
  fi
fi
