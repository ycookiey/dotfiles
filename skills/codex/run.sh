#!/usr/bin/env bash
# codex exec wrapper — 出力はログへ退避 + 要約ファイル生成
# Usage: run.sh <dir> <request> [plan_file]
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: run.sh <dir> <request> [plan_file]" >&2
  exit 2
fi

dir="${1:-.}"
request="$2"
plan_file="${3:-}"

summary_file="$dir/.codex-summary.md"
log_file="$dir/.codex-agent.log"
plan_copy="$dir/.codex-plan.md"
rm -f "$summary_file"
# planコピーは失敗・中断時も作業 repo に残さない
trap 'rm -f "$plan_copy"' EXIT

# planファイルがあればプロジェクト内にコピー
local_plan=""
if [[ -n "$plan_file" && -f "$plan_file" ]]; then
  cp "$plan_file" "$plan_copy"
  local_plan="まず .codex-plan.md を読み、その計画に従って実装せよ。"
fi

SECONDS=0
rc=0
codex exec --full-auto --cd "$dir" "${local_plan}${request}

完了後、.codex-summary.md に以下を記載:
- 変更したファイル一覧
- 各変更の要点（1行ずつ）
確認や質問は不要です。具体的な実装まで自主的に完了してください。" > "$log_file" 2>&1 || rc=$?
elapsed=$SECONDS

printf '[%dm%ds]\n' $((elapsed/60)) $((elapsed%60))
if [[ -f "$summary_file" ]]; then
  cat "$summary_file"
fi
if [[ $rc -ne 0 || ! -f "$summary_file" ]]; then
  if [[ $rc -ne 0 ]]; then
    echo "[warn] codex exec が rc=$rc で終了した。"
  fi
  if [[ ! -f "$summary_file" ]]; then
    echo "[warn] .codex-summary.md が生成されなかった。"
  fi
  echo "--- git diff --stat ---"
  git -C "$dir" diff --stat 2>/dev/null || true
  echo "--- agent log (last 20 lines) ---"
  tail -20 "$log_file" 2>/dev/null || true
fi
exit "$rc"
