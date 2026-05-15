#!/usr/bin/env bash
# agent-spawn-prep.sh — worktree作成 + .worktree-guard-config書き出し + allowlist copy
#
# 責務:
#   - worktree 作成のみ。prompt の生成・置換・永続化は Lead 側責任。
#   - prompt は Agent tool に文字列で直渡し。本 script は prompt を扱わない。
#
# 使い方:
#   agent-spawn-prep.sh --task-id <TASK_ID> [--base-ref <REF>]

set -euo pipefail

# --- 引数パース ---

TASK_ID=""
BASE_REF="HEAD"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --task-id)
      TASK_ID="$2"
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

# --- 1. repo root取得 (CWD drift対策) ---
# main worktree (= 本体 repo) を取得。cwd が worktree 内/外いずれでも同じ結果。
# 旧実装は .git ディレクトリ内で fatal となり worktree から呼ぶと cwd を返していた。
REPO_ROOT=$(git worktree list --porcelain | awk '/^worktree / {print $2; exit}')
if [[ -z "$REPO_ROOT" ]]; then
  echo "ERROR: failed to resolve main repo root via git worktree list" >&2
  exit 1
fi
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

# --- 5. .worktree-guard-config書き出し ---

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

# --- 6. allowlistによるファイルコピー ---
# global ($HOME/.claude/worktree-copy.list) と project ($REPO_ROOT/.claude/worktree-copy.list)
# を読み、glob展開してtracked**でない**ファイル/ディレクトリ(=untracked or ignored)を
# worktreeへコピー。trackedはworktreeが既に持つため除外(誤コピー防止)。
# ディレクトリの場合は配下のtrackedサブパスを除外して丸ごと持ち込む。
#
# `@junction:<pattern>` 構文 (Windows): copyの代わりにNTFS junctionを作成し、
# worktreeとmainで同一実体を共有する。大規模untracked dir (.agent-output/ 等)
# のcopy時間を回避できる。副作用: agentの書き込みがmain側に即時反映される。
# cleanup時はmerge-backのunjunction_worktreeが先にjunctionを剥がしてから
# worktreeを削除する (junctionを残したまま `git worktree remove --force` すると
# target実体まで巻き添えで削除されるため必須)。

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

# ディレクトリjunction作成: mainのsrc_dirを worktree内の同一相対パスにjunctionする。
# 既にjunction化済みなら何もしない (idempotent)。
# worktreeに同名の実ディレクトリがある場合 (tracked files が dir内にある等、
# `git worktree add` が作成したケース)、`rm -rf` で消してから junction する。
# bash の rm -rf は junction を辿らないので副作用なし。junction 越しに tracked
# files も main 側のものが同一 content で見えるので worktree の git index と
# 整合する (= git status clean)。
# mklink失敗時はcopyにfallback。
junction_one_dir() {
  local src_dir="$1"
  local dst="$WT_DIR/$src_dir"
  if [[ -L "$dst" ]]; then
    return 0
  fi
  if [[ -e "$dst" ]]; then
    if [[ -d "$dst" ]]; then
      rm -rf "$dst"
    else
      echo "WARN: $dst exists and is not a directory, cannot junction" >&2
      return 1
    fi
  fi
  mkdir -p "$(dirname "$dst")"
  local src_win dst_win
  src_win=$(cygpath -w "$REPO_ROOT/$src_dir" 2>/dev/null || echo "$REPO_ROOT/$src_dir")
  dst_win=$(cygpath -w "$dst" 2>/dev/null || echo "$dst")
  if ! cmd //c mklink //J "$dst_win" "$src_win" >/dev/null 2>&1; then
    echo "WARN: junction failed for $src_dir, falling back to copy" >&2
    copy_one_dir "$src_dir"
    return 0
  fi
  # junction 越しに見える tracked file は content sha 一致でも stat info が
  # 毎回 mismatch し git status で false-positive "modified" になる。
  # assume-unchanged でマスク (worktree の index 限定。worktree 破棄で消える)。
  local tp
  while IFS= read -r -d '' tp; do
    git -C "$WT_DIR" update-index --assume-unchanged -- "$tp" 2>/dev/null || true
  done < <(git -C "$REPO_ROOT" ls-files -z -- "$src_dir")
}

copy_from_allowlist() {
  local list_file="$1"
  [[ -f "$list_file" ]] || return 0

  local raw_line pattern src want_junction
  local -a matched
  while IFS= read -r raw_line || [[ -n "$raw_line" ]]; do
    pattern="${raw_line%%#*}"
    pattern="${pattern#"${pattern%%[![:space:]]*}"}"
    pattern="${pattern%"${pattern##*[![:space:]]}"}"
    [[ -z "$pattern" ]] && continue

    want_junction=0
    if [[ "$pattern" == "@junction:"* ]]; then
      want_junction=1
      pattern="${pattern#@junction:}"
    fi
    pattern="${pattern%/}"

    shopt -s nullglob dotglob
    # shellcheck disable=SC2206
    matched=( $pattern )
    shopt -u nullglob dotglob

    for src in "${matched[@]}"; do
      if [[ -d "$src" ]]; then
        if [[ "$want_junction" -eq 1 ]]; then
          junction_one_dir "$src"
        else
          copy_one_dir "$src"
        fi
      elif [[ -f "$src" ]]; then
        if [[ "$want_junction" -eq 1 ]]; then
          echo "WARN: @junction: applies to dirs only, skipping file: $src" >&2
        else
          copy_one_file "$src"
        fi
      fi
    done
  done < "$list_file"
}

(
  cd "$REPO_ROOT"
  copy_from_allowlist "$HOME/.claude/worktree-copy.list"
  copy_from_allowlist "$REPO_ROOT/.claude/worktree-copy.list"
)

# --- 7. project init hook (任意) ---
# <repo>/.claude/worktree-init.sh が存在すれば worktree内で実行。
# 典型用途: pnpm install --frozen-lockfile, env restore, 重量ビルド成果物link等。
# 失敗時は spawn 中断 (member が壊れた worktree で動くより明示エラーが安全)。
INIT_HOOK="$REPO_ROOT/.claude/worktree-init.sh"
if [[ -f "$INIT_HOOK" ]]; then
  echo "INFO: running project worktree-init.sh in $WT_DIR" >&2
  if ! ( cd "$WT_DIR" && bash "$INIT_HOOK" ); then
    echo "ERROR: worktree-init.sh failed (worktree kept at $WT_DIR for inspection)" >&2
    exit 1
  fi
fi

# --- 8. worktree root pathを出力 ---

WT_WIN=$(cygpath -w "$WT_DIR" 2>/dev/null || echo "$WT_DIR")
echo "WORKTREE_ROOT=$WT_DIR"
echo "WORKTREE_ROOT_WIN=$WT_WIN"
