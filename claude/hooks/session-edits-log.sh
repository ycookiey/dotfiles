#!/bin/bash
# PostToolUse hook: セッション内で編集したファイル一覧を記録
# $TMPDIR/claude-session-edits/<session_id> に1行1パス(Unix形式)で追記
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

[ -z "$FILE_PATH" ] && exit 0
[ -z "$SESSION_ID" ] && exit 0

# Windows path (C:\...) -> Unix path
if [[ "$FILE_PATH" =~ ^[A-Za-z]:\\ ]]; then
  FILE_PATH=$(cygpath -u "$FILE_PATH")
fi

DIR="${TMPDIR:-/tmp}/claude-session-edits"
mkdir -p "$DIR"
FILE="$DIR/$SESSION_ID"

# 既に記録済みならスキップ
grep -qxF "$FILE_PATH" "$FILE" 2>/dev/null || echo "$FILE_PATH" >> "$FILE"
exit 0
