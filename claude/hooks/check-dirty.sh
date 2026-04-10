#!/bin/bash
# Edit/Write pre-hook: warn Claude if file has uncommitted git changes
# 当セッションで既に編集済みのファイルは Claude 自身の変更なので警告しない。
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

[ -z "$FILE_PATH" ] && exit 0

# Convert Windows path (C:\...) to Unix path for git
if [[ "$FILE_PATH" =~ ^[A-Za-z]:\\ ]]; then
  FILE_PATH=$(cygpath -u "$FILE_PATH")
fi

# 当セッションで既に編集済みなら dirty は Claude 自身の編集 → 警告不要
EDITS_FILE="${TMPDIR:-/tmp}/claude-session-edits/${SESSION_ID:-default}"
if [ -f "$EDITS_FILE" ] && grep -qxF "$FILE_PATH" "$EDITS_FILE" 2>/dev/null; then
  exit 0
fi

# Check if file is in a git repo
REPO_DIR=$(cd "$(dirname "$FILE_PATH")" 2>/dev/null && git rev-parse --show-toplevel 2>/dev/null)
[ -z "$REPO_DIR" ] && exit 0
REPO_DIR=$(cygpath -u "$REPO_DIR" 2>/dev/null || echo "$REPO_DIR")

# Get relative path for git
REL_PATH="${FILE_PATH#"$REPO_DIR"/}"
[ "$REL_PATH" = "$FILE_PATH" ] && exit 0

# Check for unstaged and staged changes
UNSTAGED=$(git -C "$REPO_DIR" diff --stat -- "$REL_PATH" 2>/dev/null)
STAGED=$(git -C "$REPO_DIR" diff --cached --stat -- "$REL_PATH" 2>/dev/null)

[ -z "$UNSTAGED" ] && [ -z "$STAGED" ] && exit 0

# Build warning message
MSG="This file has uncommitted changes"
if [ -n "$STAGED" ]; then
  MSG="$MSG (includes staged)"
fi
MSG="$MSG — editing may mix unrelated changes into a single commit. Consider committing first."

jq -n --arg msg "$MSG" \
  '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "allow", additionalContext: $msg}}'
exit 0
