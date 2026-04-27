#!/usr/bin/env bats
# agent-spawn-prep.test.bats — TDD: 実装前のRedテスト
#
# 実行手順:
#   bats C:/Main/Project/dotfiles/skills/urleader/tests/agent-spawn-prep.test.bats
#
# bats-coreがなければ: scoop install bats  または  npm install -g bats
# Git Bashで実行可能。実装ファイル(agent-spawn-prep.sh)は未存在なのでほぼ全テストがRed。
#
# テスト対象: C:/Main/Project/dotfiles/skills/urleader/scripts/agent-spawn-prep.sh
# 仕様参照:  C:/Main/Project/AnalyzerCH/.agent-output/worktree-isolation/plan-v2.md Section 2

SCRIPT="C:/Main/Project/dotfiles/skills/urleader/scripts/agent-spawn-prep.sh"

# --- テスト共通セットアップ ---

setup() {
  # テスト用gitリポジトリを一時ディレクトリに作成
  TEST_REPO=$(mktemp -d)
  git -C "$TEST_REPO" init -q
  git -C "$TEST_REPO" config user.email "test@example.com"
  git -C "$TEST_REPO" config user.name "Test"
  # 最低1コミットが必要(HEADが解決できるように)
  touch "$TEST_REPO/README.md"
  git -C "$TEST_REPO" add README.md
  git -C "$TEST_REPO" commit -q -m "init"
  # スクリプトがCWDのgitリポジトリを使うため、TEST_REPOに移動
  cd "$TEST_REPO"
  # テスト用prompt fileを用意
  PROMPT_FILE=$(mktemp)
  # REPO_ROOT (POSIX形式) を書き込む
  REPO_POSIX=$(cygpath -u "$TEST_REPO" 2>/dev/null || echo "$TEST_REPO")
  echo "作業ディレクトリ: ${REPO_POSIX}/src/main.ts" > "$PROMPT_FILE"
}

teardown() {
  # worktreeがあれば削除してからリポジトリ削除
  if [ -d "$TEST_REPO/.claude/worktrees" ]; then
    for wt_dir in "$TEST_REPO/.claude/worktrees"/*/; do
      [ -d "$wt_dir" ] || continue
      wt_name=$(basename "$wt_dir")
      git -C "$TEST_REPO" worktree remove "$wt_dir" --force 2>/dev/null || true
      git -C "$TEST_REPO" branch -D "worktree-$wt_name" 2>/dev/null || true
    done
  fi
  rm -rf "$TEST_REPO"
  rm -f "$PROMPT_FILE"
}

# ================================================================
# 1. 正常系: worktreeディレクトリが作成される
# ================================================================
@test "1: agent-nameを渡すとworktreeディレクトリが作成される" {
  TASK_ID="T-test-1"
  run bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_FILE"
  cd "$TEST_REPO" || true  # teardownのgit操作のためCWDを設定
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  [ -d "$WT_DIR" ]
}

# ================================================================
# 2. 命名規約: pathが <repo>/.claude/worktrees/agent-<TASK_ID> 形式
# ================================================================
@test "2: worktreeのpathが命名規約に一致する" {
  TASK_ID="T-naming-check"
  run bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_FILE"
  # stdoutにWORKTREE_ROOT=...が含まれる
  echo "$output" | grep -q "WORKTREE_ROOT="
  WT_LINE=$(echo "$output" | grep "^WORKTREE_ROOT=")
  WT_PATH="${WT_LINE#WORKTREE_ROOT=}"
  # パスが .claude/worktrees/agent-<TASK_ID> で終わる
  echo "$WT_PATH" | grep -qE "\.claude/worktrees/agent-${TASK_ID}$"
}

# ================================================================
# 3. base commit: worktreeのHEADが呼び出し時のmain HEADと一致
# ================================================================
@test "3: 作成されたworktreeのHEADがmain HEADと一致する" {
  TASK_ID="T-head-check"
  MAIN_HEAD=$(git -C "$TEST_REPO" rev-parse HEAD)
  run bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_FILE"
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  WT_HEAD=$(git -C "$WT_DIR" rev-parse HEAD)
  [ "$WT_HEAD" = "$MAIN_HEAD" ]
}

# ================================================================
# 4. .worktree-guard-config: schemaキーが存在する
# ================================================================
@test "4: .worktree-guard-configにWORKTREE_ROOT/REPO_ROOT/MODEが含まれる" {
  TASK_ID="T-config-schema"
  run bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_FILE"
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  CONFIG="$WT_DIR/.worktree-guard-config"
  [ -f "$CONFIG" ]
  grep -q "^WORKTREE_ROOT=" "$CONFIG"
  grep -q "^REPO_ROOT=" "$CONFIG"
  grep -q "^MODE=" "$CONFIG"
}

# ================================================================
# 5. 衝突: 同名task-idで再実行 → 警告してreuse(exit 0)
# ================================================================
@test "5: 同名task-idで再実行するとexit 0で再利用される" {
  TASK_ID="T-dup-check"
  bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_FILE"
  # 2回目の実行
  run bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_FILE"
  [ "$status" -eq 0 ]
  # stderrにWARNが出る
  echo "$output" | grep -qi "warn" || echo "$stderr" | grep -qi "warn"
}

# ================================================================
# 6. git common dir解決: worktreeからのgit rev-parse --git-common-dir
#    がmain側.gitを指す
# ================================================================
@test "6: worktree内からgit rev-parse --git-common-dirがmainの.gitを指す" {
  TASK_ID="T-common-dir"
  run bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_FILE"
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  COMMON_DIR=$(git -C "$WT_DIR" rev-parse --git-common-dir)
  # --git-common-dirの結果はworktree内の.gitファイルではなく main側 .git ディレクトリを指す
  # .git はファイル(worktree)またはディレクトリ(main)
  # main側の .git は TEST_REPO/.git (ディレクトリ)
  MAIN_GIT_DIR=$(cygpath -u "$TEST_REPO/.git" 2>/dev/null || echo "$TEST_REPO/.git")
  COMMON_DIR_NORM=$(cygpath -u "$COMMON_DIR" 2>/dev/null || echo "$COMMON_DIR")
  [ "$COMMON_DIR_NORM" = "$MAIN_GIT_DIR" ]
}

# ================================================================
# 7. 絶対path出力: stdoutにWORKTREE_ROOT=とWORKTREE_ROOT_WIN=が含まれる
# ================================================================
@test "7: stdoutにWORKTREE_ROOTとWORKTREE_ROOT_WINの絶対pathが出力される" {
  TASK_ID="T-stdout-path"
  run bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_FILE"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "^WORKTREE_ROOT="
  echo "$output" | grep -q "^WORKTREE_ROOT_WIN="
  # WORKTREE_ROOTの値が / または C: 等で始まる絶対パスである
  WT_LINE=$(echo "$output" | grep "^WORKTREE_ROOT=")
  WT_PATH="${WT_LINE#WORKTREE_ROOT=}"
  # POSIX絶対パス (/ で始まる) または Windows形式 (C:\ や C:/) の絶対パス
  echo "$WT_PATH" | grep -qE "^(/|[A-Za-z]:)"
}

# ================================================================
# 8a. エラーケース: gitリポジトリ外で実行 → 非0終了
# ================================================================
@test "8a: gitリポジトリ外で実行すると非0で終了する" {
  NON_GIT_DIR=$(mktemp -d)
  PROMPT_FILE_TMP=$(mktemp)
  echo "test" > "$PROMPT_FILE_TMP"
  # CWDをgit外に変更して実行
  run bash -c "cd '$NON_GIT_DIR' && bash '$SCRIPT' --task-id T-nongit --prompt-file '$PROMPT_FILE_TMP'"
  rm -rf "$NON_GIT_DIR"
  rm -f "$PROMPT_FILE_TMP"
  [ "$status" -ne 0 ]
}

# ================================================================
# 8b. エラーケース: 引数欠落(--task-id未指定) → 非0終了
# ================================================================
@test "8b: --task-id引数がない場合に非0で終了する" {
  run bash "$SCRIPT" --prompt-file "$PROMPT_FILE"
  [ "$status" -ne 0 ]
}

# ================================================================
# 8c. エラーケース: 引数欠落(--prompt-file未指定) → 非0終了
# ================================================================
@test "8c: --prompt-file引数がない場合に非0で終了する" {
  run bash "$SCRIPT" --task-id "T-missing-prompt"
  [ "$status" -ne 0 ]
}

# ================================================================
# 補足: prompt file内のパス置換確認
#   (plan-v2.md Section 2 Step 5: POSIX/Win/mixed の3形式を置換)
# ================================================================
@test "9(補足): prompt file内のREPO_ROOTパスがWT_DIRに置換される" {
  TASK_ID="T-path-replace"
  REPO_POSIX=$(cygpath -u "$TEST_REPO" 2>/dev/null || echo "$TEST_REPO")
  # 3形式でパスを書き込む
  REPO_WIN=$(cygpath -w "$TEST_REPO" 2>/dev/null || echo "$TEST_REPO")
  REPO_MIX=$(cygpath -m "$TEST_REPO" 2>/dev/null || echo "$TEST_REPO")
  PROMPT_3=$(mktemp)
  printf "%s\n%s\n%s\n" "$REPO_POSIX/src" "$REPO_WIN\\src" "$REPO_MIX/src" > "$PROMPT_3"
  run bash "$SCRIPT" --task-id "$TASK_ID" --prompt-file "$PROMPT_3"
  # 置換後: REPO_ROOTの文字列が残っていないこと
  ! grep -qF "$REPO_POSIX" "$PROMPT_3"
  rm -f "$PROMPT_3"
}
