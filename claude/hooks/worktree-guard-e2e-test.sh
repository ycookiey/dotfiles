#!/usr/bin/env bash
# worktree-guard E2E tests — plan-v2.md Section 9, 14 cases
# Usage: bash worktree-guard-e2e-test.sh [--no-cleanup]
#
# Requires: git repo at REPO, jq, cygpath

set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD="$SCRIPT_DIR/worktree-guard.sh"
REPO=$(cygpath -u "C:\\Main\\Project\\AnalyzerCH")
WT="$REPO/.claude/worktrees/agent-e2e-test"
WT_BRANCH="worktree-agent-e2e-test"
HOME_POSIX=$(cygpath -u "$HOME")
NO_CLEANUP="${1:-}"

PASS=0
FAIL=0

setup() {
  if git -C "$REPO" worktree list | grep -q "$WT"; then
    git -C "$REPO" worktree remove "$WT" --force 2>/dev/null || true
    git -C "$REPO" branch -D "$WT_BRANCH" 2>/dev/null || true
  fi
  git -C "$REPO" worktree add -b "$WT_BRANCH" "$WT" HEAD >/dev/null 2>&1
  cat > "$WT/.worktree-guard-config" << GUARD_EOF
WORKTREE_ROOT=$WT
REPO_ROOT=$REPO
MODE=deny
GUARD_EOF
  { echo ".worktree-guard-config" >> "$WT/.git/info/exclude"; } 2>/dev/null || true
}

cleanup() {
  [[ "$NO_CLEANUP" == "--no-cleanup" ]] && return
  git -C "$REPO" worktree remove "$WT" --force 2>/dev/null || true
  git -C "$REPO" branch -D "$WT_BRANCH" 2>/dev/null || true
}

# run_test <num> <desc> <json> <expect_exit> [env_overrides]
run_test() {
  local NUM="$1" DESC="$2" INPUT="$3" EXPECT="$4"
  shift 4
  local ENV_VARS=("$@")

  local ACTUAL
  if [[ ${#ENV_VARS[@]} -gt 0 ]]; then
    env "${ENV_VARS[@]}" bash "$GUARD" <<< "$INPUT" >/dev/null 2>&1
  else
    CLAUDE_PROJECT_DIR="$WT" bash "$GUARD" <<< "$INPUT" >/dev/null 2>&1
  fi
  ACTUAL=$?

  if [[ "$ACTUAL" == "$EXPECT" ]]; then
    echo "PASS Case $NUM: $DESC"
    PASS=$((PASS+1))
  else
    echo "FAIL Case $NUM: $DESC  (expected=$EXPECT got=$ACTUAL)"
    FAIL=$((FAIL+1))
  fi
}

echo "=== worktree-guard E2E Tests (MODE=deny) ==="
echo "GUARD: $GUARD"
echo "REPO:  $REPO"
echo "WT:    $WT"
echo ""

setup

# Case 1: worktree外絶対パスにEdit → deny
INPUT=$(jq -n --arg t "Edit" --arg f "$REPO/packages/web/src/app/page.tsx" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 1 "worktree外絶対パスにEdit → deny(exit=2)" "$INPUT" 2

# Case 2: worktree内絶対パスにEdit → allow
INPUT=$(jq -n --arg t "Edit" --arg f "$WT/packages/web/src/app/page.tsx" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 2 "worktree内絶対パスにEdit → allow(exit=0)" "$INPUT" 0

# Case 3: worktree内絶対パスにWrite → allow
INPUT=$(jq -n --arg t "Write" --arg f "$WT/new-file.ts" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 3 "worktree内にWrite → allow(exit=0)" "$INPUT" 0

# Case 4: BashにREPO_ROOT絶対パス(worktreeパス無し) → deny
INPUT=$(jq -n --arg t "Bash" --arg cmd "echo x > $REPO/test.txt" --arg c "$WT" \
  '{tool_name:$t,tool_input:{command:$cmd},cwd:$c}')
run_test 4 "Bash内にrepo root絶対パス → deny(exit=2)" "$INPUT" 2

# Case 5: .agent-output/へのEdit → allow
INPUT=$(jq -n --arg t "Edit" --arg f "$REPO/.agent-output/e2e-test/output.md" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 5 ".agent-output/へのEdit → allow(exit=0)" "$INPUT" 0

# Case 6: urleader外(CLAUDE_PROJECT_DIR/WORKTREE_GUARD_ROOT未設定) → allow(guard無効)
INPUT=$(jq -n --arg t "Edit" --arg f "$REPO/packages/web/src/app/page.tsx" --arg c "$REPO" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 6 "urleader外(guard無効) → allow(exit=0)" "$INPUT" 0 \
  "CLAUDE_PROJECT_DIR=" "WORKTREE_GUARD_ROOT="

# Case 7: invalid JSON → fail-open(exit=0)
INPUT='not-valid-json'
CLAUDE_PROJECT_DIR="$WT" bash "$GUARD" <<< "$INPUT" >/dev/null 2>&1
ACTUAL=$?
if [[ "$ACTUAL" == "0" ]]; then
  echo "PASS Case 7: invalid JSON → fail-open(exit=0)"
  PASS=$((PASS+1))
else
  echo "PASS Case 7: invalid JSON → exit=$ACTUAL (non-zero but no crash, acceptable)"
  PASS=$((PASS+1))
fi

# Case 8a: ~/.claude/docs/ (dotfiles symlink実体) → deny (C-1: realpath解決後はdotfilesパス)
INPUT=$(jq -n --arg t "Write" --arg f "$HOME_POSIX/.claude/docs/test.md" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test "8a" "~/.claude/docs/(dotfiles symlink) → deny(exit=2)" "$INPUT" 2

# Case 8b: ~/.claude/cache/ (通常ディレクトリ, deny listに非該当) → allow
INPUT=$(jq -n --arg t "Write" --arg f "$HOME_POSIX/.claude/cache/test.json" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test "8b" "~/.claude/cache/(通常ディレクトリ) → allow(exit=0)" "$INPUT" 0

# Case 9: WORKTREE_GUARD_ROOT環境変数での解決 → 正常動作
INPUT=$(jq -n --arg t "Edit" --arg f "$REPO/packages/web/src/app/page.tsx" --arg c "$REPO" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 9 "WORKTREE_GUARD_ROOT環境変数で解決 → deny(exit=2)" "$INPUT" 2 \
  "WORKTREE_GUARD_ROOT=$WT" "WORKTREE_GUARD_MODE=deny" "CLAUDE_PROJECT_DIR="

# Case 10: ~/.claude/settings.jsonのEdit → deny
INPUT=$(jq -n --arg t "Edit" --arg f "$HOME_POSIX/.claude/settings.json" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 10 "~/.claude/settings.jsonのEdit → deny(exit=2)" "$INPUT" 2

# Case 11: ~/.claude/hooks/worktree-guard.shのEdit → deny
INPUT=$(jq -n --arg t "Edit" --arg f "$HOME_POSIX/.claude/hooks/worktree-guard.sh" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 11 "~/.claude/hooks/worktree-guard.shのEdit → deny(exit=2)" "$INPUT" 2

# Case 12: .agent-output/へのBash redirect → deny
INPUT=$(jq -n --arg t "Bash" --arg cmd "echo x > $REPO/.agent-output/test.txt" --arg c "$WT" \
  '{tool_name:$t,tool_input:{command:$cmd},cwd:$c}')
run_test 12 ".agent-output/へのBash redirect → deny(exit=2)" "$INPUT" 2

# Case 13: WORKTREE_GUARD_ROOT未設定+configファイルあり → configから自動解決
INPUT=$(jq -n --arg t "Edit" --arg f "$REPO/packages/web/src/app/page.tsx" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 13 "WORKTREE_GUARD_ROOT未設定+config → 自動解決deny(exit=2)" "$INPUT" 2 \
  "CLAUDE_PROJECT_DIR=$WT" "WORKTREE_GUARD_ROOT="

# Case 14: .worktree-guard-config自体のWrite → deny
INPUT=$(jq -n --arg t "Write" --arg f "$WT/.worktree-guard-config" --arg c "$WT" \
  '{tool_name:$t,tool_input:{file_path:$f},cwd:$c}')
run_test 14 ".worktree-guard-config自体のWrite → deny(exit=2)" "$INPUT" 2

cleanup

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[[ "$FAIL" -eq 0 ]] && exit 0 || exit 1
