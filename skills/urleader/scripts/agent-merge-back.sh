#!/usr/bin/env bash
# agent-merge-back.sh — worktree branchをmainへ取り込みcleanupする
#
# 仕様:
#   1. worktree内で `git rebase <main>` を実行（diverge解消）
#   2. main側で `git merge --ff-only <branch>` で取り込み
#   3. 成功時のみ worktree remove + branch -D で cleanup
#
# 失敗時挙動:
#   - rebase conflict: `git rebase --abort` で worktree を元に戻し非0 exit
#   - ff merge失敗: そのまま非0 exit (rebase後なのでff不可は通常起きないが保険)
#   - cleanupはmerge成功時のみ実行 (失敗時は worktree/branch を保持)
#
# pushは行わない (現方針)。
#
# 使い方:
#   agent-merge-back.sh --task-id <TASK_ID> [--main <BRANCH>]

set -euo pipefail

TASK_ID=""
MAIN_BRANCH="main"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --task-id)
      TASK_ID="$2"
      shift 2
      ;;
    --main)
      MAIN_BRANCH="$2"
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

# --- repo root ---
REPO_ROOT=$(git -C "$(git rev-parse --git-common-dir 2>/dev/null)" rev-parse --show-toplevel 2>/dev/null \
  || git rev-parse --show-toplevel)
REPO_ROOT=$(cygpath -u "$REPO_ROOT" 2>/dev/null || echo "$REPO_ROOT")

WT_DIR="$REPO_ROOT/.claude/worktrees/agent-$TASK_ID"
WT_BRANCH="worktree-agent-$TASK_ID"

if [[ ! -d "$WT_DIR" ]]; then
  echo "ERROR: worktree not found: $WT_DIR" >&2
  exit 1
fi

if ! git -C "$REPO_ROOT" show-ref --verify --quiet "refs/heads/$WT_BRANCH"; then
  echo "ERROR: branch not found: $WT_BRANCH" >&2
  exit 1
fi

if ! git -C "$REPO_ROOT" show-ref --verify --quiet "refs/heads/$MAIN_BRANCH"; then
  echo "ERROR: main branch not found: $MAIN_BRANCH" >&2
  exit 1
fi

# --- 1. worktree内でrebase ---
echo "[merge-back] rebasing $WT_BRANCH onto $MAIN_BRANCH"
if ! git -C "$WT_DIR" rebase "$MAIN_BRANCH"; then
  echo "ERROR: rebase failed (conflict). Aborting to leave worktree clean." >&2
  git -C "$WT_DIR" rebase --abort || true
  echo "Lead must resolve conflict manually or re-spawn the task." >&2
  exit 1
fi

# --- 2. main側でff-only merge ---
CUR_BRANCH=$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD)
if [[ "$CUR_BRANCH" != "$MAIN_BRANCH" ]]; then
  echo "[merge-back] checking out $MAIN_BRANCH (was $CUR_BRANCH)"
  git -C "$REPO_ROOT" checkout "$MAIN_BRANCH"
fi

echo "[merge-back] ff-only merging $WT_BRANCH into $MAIN_BRANCH"
if ! git -C "$REPO_ROOT" merge --ff-only "$WT_BRANCH"; then
  echo "ERROR: ff-only merge failed unexpectedly. worktree/branch retained for inspection." >&2
  exit 1
fi

# --- 3. cleanup (merge成功時のみ) ---
echo "[merge-back] cleanup: removing worktree and branch"
git -C "$REPO_ROOT" worktree remove "$WT_DIR" --force
git -C "$REPO_ROOT" branch -D "$WT_BRANCH"

echo "[merge-back] done: $WT_BRANCH merged into $MAIN_BRANCH"
