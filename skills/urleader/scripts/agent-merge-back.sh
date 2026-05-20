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
# main worktree (= 本体 repo) を取得。cwd が worktree 内/外いずれでも同じ結果。
# 旧実装の `git -C $(git rev-parse --git-common-dir) rev-parse --show-toplevel`
# は .git ディレクトリ内では fatal となり、|| フォールバックで cwd の
# show-toplevel が採用されて worktree path を main repo として誤認していた。
REPO_ROOT=$(git worktree list --porcelain | awk '/^worktree / {print $2; exit}')
if [[ -z "$REPO_ROOT" ]]; then
  echo "ERROR: failed to resolve main repo root via git worktree list" >&2
  exit 1
fi
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
# Windowsでは worktree内の node_modules 等で MAX_PATH (260) を超え
# `git worktree remove` がファイル削除に失敗することがある。
# その場合は PowerShell の `\\?\` 長パスprefixで強制削除し prune する。
#
# 重要: worktree内に NTFS junction (agent-spawn-prep.sh の @junction:
# allowlist で作成) が残ったまま削除すると junction を辿って main 側の実体
# まで削除される (PowerShell `Remove-Item -Recurse` は reparse point を辿る)。
# cleanup 前に unjunction_worktree() で `cmd /c rmdir` (junction 単体のみ剥がし
# 実体を辿らない安全な削除) を実行する。
#
# junction は Git Bash の `find -type l` では検出できない (NTFS junction は
# `-type d` = 通常ディレクトリとして見える)。PowerShell の ReparsePoint+
# Directory 属性で正確に列挙する。PowerShell 不在時は `find -type d -links 1`
# (junction は nlink=1) でフォールバック。
unjunction_worktree() {
  local wt="$1"
  [[ -d "$wt" ]] || return 0
  if command -v powershell.exe >/dev/null 2>&1; then
    local wt_win
    wt_win=$(cygpath -w "$wt" 2>/dev/null || echo "$wt")
    powershell.exe -NoProfile -Command "
      Get-ChildItem -LiteralPath '$wt_win' -Recurse -Force -ErrorAction SilentlyContinue |
      Where-Object { (\$_.Attributes -band [IO.FileAttributes]::ReparsePoint) -and (\$_.Attributes -band [IO.FileAttributes]::Directory) } |
      Sort-Object FullName -Descending |
      ForEach-Object { cmd /c rmdir \"\$(\$_.FullName)\" }
    " >/dev/null 2>&1 || true
  else
    local link link_win
    while IFS= read -r -d '' link; do
      link_win=$(cygpath -w "$link" 2>/dev/null || echo "$link")
      cmd //c rmdir "$link_win" >/dev/null 2>&1 || true
    done < <(find "$wt" -type d -links 1 -print0 2>/dev/null)
  fi
}

cleanup_worktree() {
  if git -C "$REPO_ROOT" worktree remove "$WT_DIR" --force 2>/dev/null; then
    return 0
  fi
  echo "[merge-back] worktree remove failed (likely Windows MAX_PATH); retrying with long-path prefix" >&2
  # 二重防衛: リトライ削除 (Remove-Item -Recurse / rm -rf) が junction を辿らない
  # よう、再帰削除の直前にも junction を確実に剥がす。
  unjunction_worktree "$WT_DIR"
  local wt_win
  wt_win=$(cygpath -w "$WT_DIR" 2>/dev/null || echo "$WT_DIR")
  if command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -Command "Remove-Item -LiteralPath '\\\\?\\$wt_win' -Recurse -Force -ErrorAction Stop"
  else
    rm -rf "$WT_DIR"
  fi
  git -C "$REPO_ROOT" worktree prune
}

echo "[merge-back] cleanup: removing worktree and branch"
unjunction_worktree "$WT_DIR"
cleanup_worktree
git -C "$REPO_ROOT" branch -D "$WT_BRANCH"

echo "[merge-back] done: $WT_BRANCH merged into $MAIN_BRANCH"
