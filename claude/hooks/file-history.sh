#!/bin/bash
# PostToolUse hook: 編集成功時にファイルのスナップショットを非同期保存
# ~/.file-history/{unix_path}/{epoch}.bak
INPUT=$(cat)

TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty')
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

[ -z "$FILE_PATH" ] && exit 0

# Write の新規作成は履歴不要（保存すべき旧版がない）
[ "$TOOL" = "Write" ] && [ ! -f "$FILE_PATH" ] && exit 0

# Windows path -> Unix path
if [[ "$FILE_PATH" =~ ^[A-Za-z]:\\ ]]; then
  FILE_PATH=$(cygpath -u "$FILE_PATH")
fi

HISTORY_DIR="$HOME/.file-history${FILE_PATH}"
mkdir -p "$HISTORY_DIR"
cp "$FILE_PATH" "$HISTORY_DIR/$(date +%s).bak" &
exit 0
