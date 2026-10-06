#!/usr/bin/env bash
# devserver-launch-guard.sh — PreToolUse hook (Bash):
# Git Bash の run_in_background で常駐の dev server(pnpm dev・vite・wrangler dev 等)を起動すると、
# npm の global の shim(sh スクリプト)を MSYS の fork/exec で動かす所で Windows の親子のつながりが切れ、
# TaskStop が木をたどれず node・workerd が残って port を掴み続ける。PowerShell の tool で起動すると
# つながりが保たれ、TaskStop で全部止まる。その形の起動だけを止め、PowerShell での起動を返す。
# 詳細 ~/.claude/docs/bash.md の「dev server は PowerShell の tool で起動する」
input=$(cat)
[ "$(jq -r '.tool_input.run_in_background // false' <<<"$input")" = "true" ] || exit 0
command=$(jq -r '.tool_input.command // ""' <<<"$input")
start='(^|[;&|(`[:space:]])'
end='([[:space:];&|)]|$)'
pm="${start}(pnpm|npm|yarn|bun)([[:space:]]+(run|exec|-[^[:space:]]+([[:space:]]+[^-;&|[:space:]][^;&|[:space:]]*)?))*[[:space:]]+(dev|start|serve|preview|storybook)${end}"
# vite 単体・オプション付き・dev/serve/preview は dev server。vite build は通す
tool="${start}(npx[[:space:]]+)?(vite([[:space:]]+(dev|serve|preview)${end}|[[:space:]]+-|[[:space:]]*([;&|)]|$))|(wrangler[[:space:]]+dev|next[[:space:]]+(dev|start)|storybook[[:space:]]+dev)${end})"
grep -Eq "$pm|$tool" <<<"$command" || exit 0
cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"常駐の dev server は Bash ではなく PowerShell の tool の run_in_background で起動する(例: Set-Location C:/path/app; pnpm dev --port 5190 --strictPort *> <log>)。Git Bash から起動すると npm の shim の所で親子のつながりが切れ、TaskStop の後も node・workerd が残って port を掴む。詳細 ~/.claude/docs/bash.md の「dev server は PowerShell の tool で起動する」"}}
JSON
