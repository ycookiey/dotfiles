#!/usr/bin/env bash
# codex exec wrapper — stdout破棄 + 要約ファイル生成
# Usage: run.sh <dir> <request> [plan_file]
set -euo pipefail

dir="${1:-.}"
request="$2"
plan_file="${3:-}"

summary_file="$dir/.codex-summary.md"
rm -f "$summary_file"

# planファイルがあればプロジェクト内にコピー
local_plan=""
if [[ -n "$plan_file" && -f "$plan_file" ]]; then
  cp "$plan_file" "$dir/.codex-plan.md"
  local_plan="まず .codex-plan.md を読み、その計画に従って実装せよ。"
fi

codex exec --full-auto --cd "$dir" "${local_plan}${request}

完了後、.codex-summary.md に以下を記載:
- 変更したファイル一覧
- 各変更の要点（1行ずつ）
確認や質問は不要です。具体的な実装まで自主的に完了してください。" > /dev/null 2>&1

# planコピーを削除
rm -f "$dir/.codex-plan.md"

if [[ -f "$summary_file" ]]; then
  cat "$summary_file"
else
  echo "[warn] .codex-summary.md が生成されなかった。git diff --stat:"
  git -C "$dir" diff --stat
fi
