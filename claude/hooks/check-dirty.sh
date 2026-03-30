#!/bin/bash
# Edit/Write pre-hook: warn Claude if file has uncommitted git changes
# Warns once per file per session to avoid repeated noise.
INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

[ -z "$FILE_PATH" ] && exit 0

# Convert Windows path (C:\...) to Unix path for git
if [[ "$FILE_PATH" =~ ^[A-Za-z]:\\ ]]; then
  FILE_PATH=$(cygpath -u "$FILE_PATH")
fi

# Skip if already warned in this session
WARNED_DIR="${TMPDIR:-/tmp}/claude-dirty-check"
WARNED_FILE="$WARNED_DIR/${SESSION_ID:-default}"
mkdir -p "$WARNED_DIR"
if [ -f "$WARNED_FILE" ] && grep -qxF "$FILE_PATH" "$WARNED_FILE" 2>/dev/null; then
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

# Record this file as warned
echo "$FILE_PATH" >> "$WARNED_FILE"

# Build warning message
MSG="This file has uncommitted changes"
if [ -n "$STAGED" ]; then
  MSG="$MSG (includes staged)"
fi
MSG="$MSG — editing may mix unrelated changes into a single commit. Consider committing first."

jq -n --arg msg "$MSG" \
  '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "allow", additionalContext: $msg}}'
exit 0
