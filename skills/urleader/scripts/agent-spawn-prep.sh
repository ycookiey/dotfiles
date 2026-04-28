#!/usr/bin/env bash
# agent-spawn-prep.sh — worktree作成 + prompt pathsの書き換え + .worktree-guard-config書き出し
# 仕様: .agent-output/worktree-isolation/plan-v2.md Section 2
#
# 使い方:
#   agent-spawn-prep.sh --task-id <TASK_ID> --prompt-file <PATH> [--base-ref <REF>]

set -euo pipefail

# --- 引数パース ---

TASK_ID=""
PROMPT_FILE=""
BASE_REF="HEAD"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --task-id)
      TASK_ID="$2"
      shift 2
      ;;
    --prompt-file)
      PROMPT_FILE="$2"
      shift 2
      ;;
    --base-ref)
      BASE_REF="$2"
      shift 2
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$TASK_ID" ]]; then
  echo "ERROR: --task-id is required" >&2
  exit 1
fi

if [[ -z "$PROMPT_FILE" ]]; then
  echo "ERROR: --prompt-file is required" >&2
  exit 1
fi

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "ERROR: prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

# --- 1. repo root取得 (CWD drift対策) ---

REPO_ROOT=$(git -C "$(git rev-parse --git-common-dir 2>/dev/null)" rev-parse --show-toplevel 2>/dev/null \
  || git rev-parse --show-toplevel)
REPO_ROOT=$(cygpath -u "$REPO_ROOT" 2>/dev/null || echo "$REPO_ROOT")

# --- 2. worktreeディレクトリとbranch名の決定 ---

WT_DIR="$REPO_ROOT/.claude/worktrees/agent-$TASK_ID"
WT_BRANCH="worktree-agent-$TASK_ID"

# --- 3. 既存worktree確認 ---

if [[ -d "$WT_DIR" ]]; then
  echo "WARN: worktree already exists at $WT_DIR, reusing" >&2
fi

# --- 4. worktree作成 ---

if [[ ! -d "$WT_DIR" ]]; then
  mkdir -p "$(dirname "$WT_DIR")"
  git -C "$REPO_ROOT" worktree add -b "$WT_BRANCH" "$WT_DIR" "$BASE_REF"
fi

# --- 5. prompt file内の絶対パス置換 ---

REPO_WIN=$(cygpath -w "$REPO_ROOT" 2>/dev/null || echo "$REPO_ROOT")
REPO_MIX=$(cygpath -m "$REPO_ROOT" 2>/dev/null || echo "$REPO_ROOT")
WT_WIN=$(cygpath -w "$WT_DIR" 2>/dev/null || echo "$WT_DIR")
WT_MIX=$(cygpath -m "$WT_DIR" 2>/dev/null || echo "$WT_DIR")

# 空ファイルはスキップ
if [[ -s "$PROMPT_FILE" ]]; then
  PROMPT_CONTENT=$(cat "$PROMPT_FILE")

  # Windows形式のバックスラッシュをsedでエスケープして置換
  REPO_WIN_ESC="${REPO_WIN//\\/\\\\}"
  WT_WIN_ESC="${WT_WIN//\\/\\\\}"

  PROMPT_CONTENT=$(printf '%s' "$PROMPT_CONTENT" | sed "s|${REPO_WIN_ESC}|${WT_WIN_ESC}|g")
  PROMPT_CONTENT=$(printf '%s' "$PROMPT_CONTENT" | sed "s|${REPO_MIX}|${WT_MIX}|g")
  PROMPT_CONTENT=$(printf '%s' "$PROMPT_CONTENT" | sed "s|${REPO_ROOT}|${WT_DIR}|g")

  printf '%s' "$PROMPT_CONTENT" > "$PROMPT_FILE"

  # --- 6. 置換漏れ検出 (ゲート) ---
  # WT_DIR以下のパスはOK (REPO_ROOTはWT_DIRのprefixとして含まれる場合がある)
  LEAK_POSIX=$(grep -F "$REPO_ROOT" "$PROMPT_FILE" | grep -vF "$WT_DIR" || true)
  LEAK_WIN=$(grep -F "$REPO_WIN" "$PROMPT_FILE" | grep -vF "$WT_WIN" || true)
  LEAK_MIX=$(grep -F "$REPO_MIX" "$PROMPT_FILE" | grep -vF "$WT_MIX" || true)
  if [[ -n "$LEAK_POSIX" || -n "$LEAK_WIN" || -n "$LEAK_MIX" ]]; then
    echo "ERROR: prompt file still contains original repo path after substitution" >&2
    echo "Remaining occurrences:" >&2
    [[ -n "$LEAK_POSIX" ]] && echo "$LEAK_POSIX" >&2
    [[ -n "$LEAK_WIN" ]] && echo "$LEAK_WIN" >&2
    [[ -n "$LEAK_MIX" ]] && echo "$LEAK_MIX" >&2
    exit 1
  fi
fi

# --- 7. .worktree-guard-config書き出し ---

cat > "$WT_DIR/.worktree-guard-config" <<GUARD_EOF
WORKTREE_ROOT=$WT_DIR
REPO_ROOT=$REPO_ROOT
MODE=warn
GUARD_EOF

# configをgit管理外に (worktreeの.gitはファイルなのでgit rev-parse --git-dirで実際のgitdirを取得)
WT_GIT_DIR=$(git -C "$WT_DIR" rev-parse --git-dir)
WT_GIT_DIR=$(cygpath -u "$WT_GIT_DIR" 2>/dev/null || echo "$WT_GIT_DIR")
mkdir -p "$WT_GIT_DIR/info"
echo ".worktree-guard-config" >> "$WT_GIT_DIR/info/exclude"

# --- 8. allowlistによるファイルコピー ---
# global ($HOME/.claude/worktree-copy.list) と project ($REPO_ROOT/.claude/worktree-copy.list)
# を読み、glob展開してtracked**でない**ファイル/ディレクトリ(=untracked or ignored)を
# worktreeへコピー。trackedはworktreeが既に持つため除外(誤コピー防止)。
# ディレクトリの場合は配下のtrackedサブパスを除外して丸ごと持ち込む。

# ファイル単体コピー: trackedならスキップ、それ以外コピー
copy_one_file() {
  local src_file="$1"
  if git -C "$REPO_ROOT" ls-files --error-unmatch -- "$src_file" >/dev/null 2>&1; then
    return 0
  fi
  local dst="$WT_DIR/$src_file"
  mkdir -p "$(dirname "$dst")"
  cp -p "$src_file" "$dst"
}

# ディレクトリコピー: 配下のtrackedサブパスを除外して非trackedファイルのみコピー
copy_one_dir() {
  local src_dir="$1"
  # tracked サブパス集合を構築 (NUL区切りで安全に扱う)
  declare -A tracked_set=()
  local tp
  while IFS= read -r -d '' tp; do
    tracked_set["$tp"]=1
  done < <(git -C "$REPO_ROOT" ls-files -z -- "$src_dir")

  local f rel
  while IFS= read -r -d '' f; do
    rel="${f#./}"
    [[ -n "${tracked_set[$rel]:-}" ]] && continue
    mkdir -p "$WT_DIR/$(dirname "$rel")"
    cp -p "$f" "$WT_DIR/$rel"
  done < <(find "$src_dir" -type f -print0)
}

copy_from_allowlist() {
  local list_file="$1"
  [[ -f "$list_file" ]] || return 0

  local raw_line pattern src
  local -a matched
  while IFS= read -r raw_line || [[ -n "$raw_line" ]]; do
    pattern="${raw_line%%#*}"
    pattern="${pattern#"${pattern%%[![:space:]]*}"}"
    pattern="${pattern%"${pattern##*[![:space:]]}"}"
    [[ -z "$pattern" ]] && continue
    pattern="${pattern%/}"

    shopt -s nullglob dotglob
    # shellcheck disable=SC2206
    matched=( $pattern )
    shopt -u nullglob dotglob

    for src in "${matched[@]}"; do
      if [[ -d "$src" ]]; then
        copy_one_dir "$src"
      elif [[ -f "$src" ]]; then
        copy_one_file "$src"
      fi
    done
  done < "$list_file"
}

(
  cd "$REPO_ROOT"
  copy_from_allowlist "$HOME/.claude/worktree-copy.list"
  copy_from_allowlist "$REPO_ROOT/.claude/worktree-copy.list"
)

# --- 9. worktree root pathを出力 ---

echo "WORKTREE_ROOT=$WT_DIR"
echo "WORKTREE_ROOT_WIN=$WT_WIN"
