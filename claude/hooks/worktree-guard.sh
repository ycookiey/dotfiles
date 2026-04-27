#!/usr/bin/env bash
# worktree-guard.sh — PreToolUse hook for worktree boundary enforcement

set -o pipefail

# --- ヘルパー関数 ---

resolve_guard_root() {
  # 1. 環境変数があればそれを使用
  if [[ -n "$WORKTREE_GUARD_ROOT" ]]; then
    echo "$WORKTREE_GUARD_ROOT"
    return 0
  fi

  # 2. CLAUDE_PROJECT_DIRから.worktree-guard-configを探す
  if [[ -n "$CLAUDE_PROJECT_DIR" ]]; then
    local CONFIG_PATH
    CONFIG_PATH=$(cygpath -u "$CLAUDE_PROJECT_DIR" 2>/dev/null || echo "$CLAUDE_PROJECT_DIR")
    CONFIG_PATH="$CONFIG_PATH/.worktree-guard-config"
    if [[ -f "$CONFIG_PATH" ]]; then
      local ROOT
      ROOT=$(grep '^WORKTREE_ROOT=' "$CONFIG_PATH" | head -1 | cut -d= -f2-)
      if [[ -n "$ROOT" ]]; then
        echo "$ROOT"
        return 0
      fi
    fi
  fi

  # 3. hook inputのcwdから.worktree-guard-configを上方探索
  local CWD_FROM_INPUT
  CWD_FROM_INPUT=$(echo "$HOOK_INPUT" | jq -r '.cwd // empty')
  if [[ -n "$CWD_FROM_INPUT" ]]; then
    local CWD_NORM
    CWD_NORM=$(cygpath -u "$CWD_FROM_INPUT" 2>/dev/null || echo "$CWD_FROM_INPUT")
    local SEARCH_DIR="$CWD_NORM"
    for _ in 1 2 3 4 5; do
      if [[ -f "$SEARCH_DIR/.worktree-guard-config" ]]; then
        local ROOT
        ROOT=$(grep '^WORKTREE_ROOT=' "$SEARCH_DIR/.worktree-guard-config" | head -1 | cut -d= -f2-)
        if [[ -n "$ROOT" ]]; then
          echo "$ROOT"
          return 0
        fi
      fi
      SEARCH_DIR=$(dirname "$SEARCH_DIR")
    done
  fi

  # 4. 見つからない → guard無効
  return 1
}

check_home_path() {
  local NORMALIZED="$1"
  local HOME_REAL="$2"

  # HOME配下でなければこの関数のスコープ外
  [[ "$NORMALIZED" != "$HOME_REAL"/* ]] && return 1

  # === 明示deny一覧 ===
  local DENY_PATTERNS=(
    "$HOME_REAL/.claude/settings.json"
    "$HOME_REAL/.claude/settings.local.json"
    "$HOME_REAL/.claude/hooks"
    "$HOME_REAL/.claude/skills/urleader"
    "*/.worktree-guard-config"
  )

  # マルチアカウント (~/.claude-*/) deny
  if [[ "$NORMALIZED" =~ ^"$HOME_REAL"/.claude-[^/]+/(settings\.json|settings\.local\.json)$ ]]; then
    return 2
  fi
  if [[ "$NORMALIZED" == "$HOME_REAL"/.claude-*/hooks/* ]] || \
     [[ "$NORMALIZED" == "$HOME_REAL"/.claude-*/skills/urleader/* ]]; then
    return 2
  fi

  # DENY_PATTERNS判定
  for DENY in "${DENY_PATTERNS[@]}"; do
    if [[ "$DENY" == "*/"* ]]; then
      local BASENAME="${DENY##*/}"
      [[ "$(basename "$NORMALIZED")" == "$BASENAME" ]] && return 2
    elif [[ -e "$DENY" ]]; then
      local DENY_REAL
      DENY_REAL=$(realpath "$DENY" 2>/dev/null || echo "$DENY")
      [[ "$NORMALIZED" == "$DENY_REAL" ]] && return 2
      [[ -d "$DENY" && "$NORMALIZED" == "$DENY_REAL"/* ]] && return 2
    else
      # 存在しないパスでも文字列比較でdeny
      [[ "$NORMALIZED" == "$DENY" ]] && return 2
      [[ "$NORMALIZED" == "$DENY"/* ]] && return 2
    fi
  done

  # deny一覧に該当しないHOME配下は許可
  return 0
}

deny_path() {
  local FILE_PATH="$1"
  if [[ "$MODE" == "warn" ]]; then
    echo "WARN: would block path '$FILE_PATH' (outside worktree '$GUARD_ROOT')" >&2
    exit 0
  else
    echo "BLOCKED: file path '$FILE_PATH' is outside worktree boundary '$GUARD_ROOT'" >&2
    exit 2
  fi
}

deny_bash() {
  local COMMAND="$1"
  local PATTERN="$2"
  if [[ "$MODE" == "warn" ]]; then
    echo "WARN: would block Bash command containing repo root path (pattern: $PATTERN)" >&2
    exit 0
  else
    echo "BLOCKED: Bash command contains repo root path outside worktree" >&2
    echo "  pattern: $PATTERN" >&2
    exit 2
  fi
}

handle_bash() {
  local INPUT="$1"
  local COMMAND
  COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command')

  local GUARD_NORM
  GUARD_NORM=$(cygpath -u "$GUARD_ROOT" 2>/dev/null || echo "$GUARD_ROOT")
  GUARD_NORM=$(realpath "$GUARD_NORM" 2>/dev/null || echo "$GUARD_NORM")

  local REPO_ROOT_BASH="$REPO_ROOT_RESOLVED"
  if [[ -z "$REPO_ROOT_BASH" ]]; then
    REPO_ROOT_BASH=$(git -C "$GUARD_NORM" rev-parse --show-toplevel 2>/dev/null)
    REPO_ROOT_BASH=$(cygpath -u "$REPO_ROOT_BASH" 2>/dev/null || echo "$REPO_ROOT_BASH")
  fi

  local REPO_POSIX REPO_WIN REPO_MIX
  REPO_POSIX=$(cygpath -u "$REPO_ROOT_BASH" 2>/dev/null || echo "$REPO_ROOT_BASH")
  REPO_WIN=$(cygpath -w "$REPO_ROOT_BASH" 2>/dev/null || echo "$REPO_ROOT_BASH")
  REPO_MIX=$(cygpath -m "$REPO_ROOT_BASH" 2>/dev/null || echo "$REPO_ROOT_BASH")

  local WT_POSIX WT_WIN WT_MIX
  WT_POSIX=$(cygpath -u "$GUARD_NORM" 2>/dev/null || echo "$GUARD_NORM")
  WT_WIN=$(cygpath -w "$GUARD_NORM" 2>/dev/null || echo "$GUARD_NORM")
  WT_MIX=$(cygpath -m "$GUARD_NORM" 2>/dev/null || echo "$GUARD_NORM")

  for PATTERN in "$REPO_POSIX" "$REPO_WIN" "$REPO_MIX"; do
    [[ -z "$PATTERN" ]] && continue
    if echo "$COMMAND" | grep -qF "$PATTERN"; then
      local HAS_WT=false
      for WT_PAT in "$WT_POSIX" "$WT_WIN" "$WT_MIX"; do
        echo "$COMMAND" | grep -qF "$WT_PAT" && HAS_WT=true && break
      done

      if [[ "$HAS_WT" == false ]]; then
        deny_bash "$COMMAND" "$PATTERN"
        return $?
      fi
    fi
  done

  exit 0
}

# [M-5対応] source実行禁止ガード
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then
  echo "ERROR: worktree-guard.sh must not be sourced" >&2
  return 1 2>/dev/null || exit 1
fi

# --- メインロジック ---

# 1. stdin JSON取得
HOOK_INPUT=$(cat)
TOOL_NAME=$(echo "$HOOK_INPUT" | jq -r '.tool_name')

# 2. GUARD_ROOT解決
GUARD_ROOT=$(resolve_guard_root)
if [[ -z "$GUARD_ROOT" ]]; then
  exit 0
fi

# 3. MODE読み取り
MODE="${WORKTREE_GUARD_MODE:-}"
if [[ -z "$MODE" ]]; then
  CONFIG_FOR_MODE=""
  if [[ -n "$CLAUDE_PROJECT_DIR" ]]; then
    CONFIG_FOR_MODE=$(cygpath -u "$CLAUDE_PROJECT_DIR" 2>/dev/null || echo "$CLAUDE_PROJECT_DIR")
    CONFIG_FOR_MODE="$CONFIG_FOR_MODE/.worktree-guard-config"
  fi
  if [[ -n "$CONFIG_FOR_MODE" && -f "$CONFIG_FOR_MODE" ]]; then
    MODE=$(grep '^MODE=' "$CONFIG_FOR_MODE" | head -1 | cut -d= -f2-)
  fi
fi
MODE="${MODE:-deny}"

# 4. REPO_ROOT解決
REPO_ROOT_RESOLVED=""
if [[ -n "$CLAUDE_PROJECT_DIR" ]]; then
  CP_PATH=$(cygpath -u "$CLAUDE_PROJECT_DIR" 2>/dev/null || echo "$CLAUDE_PROJECT_DIR")
  CP_PATH="$CP_PATH/.worktree-guard-config"
  [[ -f "$CP_PATH" ]] && REPO_ROOT_RESOLVED=$(grep '^REPO_ROOT=' "$CP_PATH" | head -1 | cut -d= -f2-)
fi
# CLAUDE_PROJECT_DIR未設定の場合はGUARD_ROOTのconfigから取得
if [[ -z "$REPO_ROOT_RESOLVED" ]]; then
  GUARD_CONFIG_PATH=$(cygpath -u "$GUARD_ROOT" 2>/dev/null || echo "$GUARD_ROOT")
  GUARD_CONFIG_PATH="$GUARD_CONFIG_PATH/.worktree-guard-config"
  [[ -f "$GUARD_CONFIG_PATH" ]] && REPO_ROOT_RESOLVED=$(grep '^REPO_ROOT=' "$GUARD_CONFIG_PATH" | head -1 | cut -d= -f2-)
fi
if [[ -z "$REPO_ROOT_RESOLVED" ]]; then
  GUARD_NORM_TMP=$(cygpath -u "$GUARD_ROOT" 2>/dev/null || echo "$GUARD_ROOT")
  GUARD_NORM_TMP=$(realpath "$GUARD_NORM_TMP" 2>/dev/null || echo "$GUARD_NORM_TMP")
  REPO_ROOT_RESOLVED=$(git -C "$GUARD_NORM_TMP" rev-parse --show-toplevel 2>/dev/null)
  REPO_ROOT_RESOLVED=$(cygpath -u "$REPO_ROOT_RESOLVED" 2>/dev/null || echo "$REPO_ROOT_RESOLVED")
fi

# 5. ツール別処理
case "$TOOL_NAME" in
  Edit|Write|MultiEdit)
    FILE_PATH=$(echo "$HOOK_INPUT" | jq -r '.tool_input.file_path')
    ;;
  Bash)
    handle_bash "$HOOK_INPUT"
    exit $?
    ;;
  *)
    exit 0
    ;;
esac

# 6. file_pathが空/nullなら通す
[[ -z "$FILE_PATH" || "$FILE_PATH" == "null" ]] && exit 0

# 7. path正規化
NORMALIZED=$(cygpath -u "$FILE_PATH" 2>/dev/null || echo "$FILE_PATH")
if [ -e "$NORMALIZED" ]; then
  NORMALIZED=$(realpath "$NORMALIZED" 2>/dev/null || echo "$NORMALIZED")
elif [ -e "$(dirname "$NORMALIZED")" ]; then
  NORMALIZED="$(realpath "$(dirname "$NORMALIZED")" 2>/dev/null || dirname "$NORMALIZED")/$(basename "$NORMALIZED")"
fi

# 8. GUARD_ROOT正規化
GUARD_NORM=$(cygpath -u "$GUARD_ROOT" 2>/dev/null || echo "$GUARD_ROOT")
GUARD_NORM=$(realpath "$GUARD_NORM" 2>/dev/null || echo "$GUARD_NORM")

# 9. 許可パス判定

# (pre) .worktree-guard-config は場所を問わずdeny (worktree内でも)
[[ "$(basename "$NORMALIZED")" == ".worktree-guard-config" ]] && deny_path "$FILE_PATH"

# (a) worktree内 → allow
[[ "$NORMALIZED" == "$GUARD_NORM"/* ]] && exit 0

# (b) .agent-output → allow (Edit/Write経由のみ)
REPO_ROOT_NORM=$(realpath "$REPO_ROOT_RESOLVED" 2>/dev/null || echo "$REPO_ROOT_RESOLVED")
[[ -n "$REPO_ROOT_NORM" && "$NORMALIZED" == "$REPO_ROOT_NORM/.agent-output"/* ]] && exit 0

# (c) HOME配下: ホワイトリスト方式
HOME_POSIX=$(cygpath -u "$HOME" 2>/dev/null || echo "$HOME")
HOME_REAL=$(realpath "$HOME_POSIX" 2>/dev/null || echo "$HOME_POSIX")
check_home_path "$NORMALIZED" "$HOME_REAL"
HOME_RESULT=$?
[[ $HOME_RESULT -eq 0 ]] && exit 0
[[ $HOME_RESULT -eq 2 ]] && deny_path "$FILE_PATH"

# (d) REPO_ROOT配下のファイルはここまでに処理済み(worktreeかagent-output)のみ許可
#     REPO_ROOT配下でないものは/tmp等の一時パスとして許可
if [[ -n "$REPO_ROOT_NORM" && "$NORMALIZED" == "$REPO_ROOT_NORM"/* ]]; then
  deny_path "$FILE_PATH"
fi

# (e) /tmp, /dev/null等 → allow
[[ "$NORMALIZED" == /tmp/* || "$NORMALIZED" == /dev/* ]] && exit 0

# 10. deny
deny_path "$FILE_PATH"
