#!/usr/bin/env bash
# PostToolUse hook: ツール呼び出しをログに記録（TOKEN_AUDIT_LOG=1 の時のみ）
# 通常は settings の dotcli token-audit-hook を使用。bash のみで使う場合向け。
[[ "${TOKEN_AUDIT_LOG:-}" != "1" ]] && exit 0

{
  input=$(cat)
  tool_name=$(echo "$input" | jq -r '.tool_name // "unknown"' 2>/dev/null)
  session_id=$(echo "$input" | jq -r '.session_id // "unknown"' 2>/dev/null)
  result_size=$(echo "$input" | jq -r '.tool_response // "" | tostring | length' 2>/dev/null || echo 0)
  echo -e "$(date -Is)\t${tool_name}\t${session_id}\t${result_size}" >> "$HOME/.claude/token-audit.log"
} &

exit 0
