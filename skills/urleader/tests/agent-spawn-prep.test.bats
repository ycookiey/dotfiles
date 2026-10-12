#!/usr/bin/env bats
# agent-spawn-prep.test.bats — agent-spawn-prep.sh の unit テスト
#
# 実行手順 (repo root から):
#   bats skills/urleader/tests/agent-spawn-prep.test.bats
#
# bats-coreがなければ: scoop install bats  または  npm install -g bats
# Git Bashで実行可能。
#
# テスト対象: このテストファイルと同じ checkout の skills/urleader/scripts/agent-spawn-prep.sh
# 副作用: 一時 git repo と BATS_TEST_TMPDIR 配下だけ。HOME も一時 dir に差し替えるので
#         実環境の ~/.claude/worktree-copy.list は読まない。

SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../scripts" && pwd)/agent-spawn-prep.sh"

# --- テスト共通セットアップ ---

setup() {
  # global allowlist ($HOME/.claude/worktree-copy.list) と git の global 設定を実環境から切り離す
  export HOME="$BATS_TEST_TMPDIR/home"
  mkdir -p "$HOME/.claude"
  unset XDG_CONFIG_HOME

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
}

# common dir 側の info/exclude (git が実際に読む方) の POSIX パス
common_exclude() {
  local p
  p=$(git -C "$1" rev-parse --git-path info/exclude)
  cygpath -u "$p" 2>/dev/null || echo "$p"
}

# ================================================================
# 1. 正常系: worktreeディレクトリが作成される
# ================================================================
@test "1: agent-nameを渡すとworktreeディレクトリが作成される" {
  TASK_ID="T-test-1"
  run bash "$SCRIPT" --task-id "$TASK_ID"
  cd "$TEST_REPO" || true  # teardownのgit操作のためCWDを設定
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  [ -d "$WT_DIR" ]
}

# ================================================================
# 2. 命名規約: pathが <repo>/.claude/worktrees/agent-<TASK_ID> 形式
# ================================================================
@test "2: worktreeのpathが命名規約に一致する" {
  TASK_ID="T-naming-check"
  run bash "$SCRIPT" --task-id "$TASK_ID"
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
  run bash "$SCRIPT" --task-id "$TASK_ID"
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  WT_HEAD=$(git -C "$WT_DIR" rev-parse HEAD)
  [ "$WT_HEAD" = "$MAIN_HEAD" ]
}

# ================================================================
# 4. .worktree-guard-config: schemaキーが存在する
# ================================================================
@test "4: .worktree-guard-configにWORKTREE_ROOT/REPO_ROOT/MODEが含まれる" {
  TASK_ID="T-config-schema"
  run bash "$SCRIPT" --task-id "$TASK_ID"
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
  bash "$SCRIPT" --task-id "$TASK_ID"
  # 2回目の実行 (bats の run は stderr も $output に含める)
  run bash "$SCRIPT" --task-id "$TASK_ID"
  [ "$status" -eq 0 ]
  echo "$output" | grep -qi "warn"
}

# ================================================================
# 6. git common dir解決: worktreeからのgit rev-parse --git-common-dir
#    がmain側.gitを指す
# ================================================================
@test "6: worktree内からgit rev-parse --git-common-dirがmainの.gitを指す" {
  TASK_ID="T-common-dir"
  run bash "$SCRIPT" --task-id "$TASK_ID"
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
  run bash "$SCRIPT" --task-id "$TASK_ID"
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
  # CWDをgit外に変更して実行
  run bash -c "cd '$NON_GIT_DIR' && bash '$SCRIPT' --task-id T-nongit"
  rm -rf "$NON_GIT_DIR"
  [ "$status" -ne 0 ]
}

# ================================================================
# 8b. エラーケース: 引数なし → --task-id 必須エラーで非0終了
# ================================================================
@test "8b: 引数なしで実行すると--task-id必須エラーで非0終了する" {
  run bash "$SCRIPT"
  [ "$status" -ne 0 ]
  [[ "$output" == *"--task-id is required"* ]]
}

# ================================================================
# 8c. エラーケース: 他の引数はあるが --task-id がない → 非0終了
# ================================================================
@test "8c: --base-refだけ渡し--task-idがない場合に--task-id必須エラーで非0終了する" {
  run bash "$SCRIPT" --base-ref HEAD
  [ "$status" -ne 0 ]
  [[ "$output" == *"--task-id is required"* ]]
  [ ! -d "$TEST_REPO/.claude/worktrees" ]
}

# ================================================================
# 8d. エラーケース: 廃止済みの --prompt-file は unknown argument で拒否
# ================================================================
@test "8d: 廃止した--prompt-fileを渡すとunknown argumentで非0終了する" {
  run bash "$SCRIPT" --task-id "T-removed-flag" --prompt-file /dev/null
  [ "$status" -ne 0 ]
  [[ "$output" == *"unknown argument"* ]]
}

# ================================================================
# 10. .worktree-guard-config は worktree の git status に出ない
#     (common 側 info/exclude に書かれていること。worktree 固有の gitdir 側は git が読まない)
# ================================================================
@test "10: worktreeのgit statusに.worktree-guard-configが出ない" {
  TASK_ID="T-exclude"
  run bash "$SCRIPT" --task-id "$TASK_ID"
  [ "$status" -eq 0 ]
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  [ -f "$WT_DIR/.worktree-guard-config" ]
  run git -C "$WT_DIR" status --porcelain
  [ "$status" -eq 0 ]
  echo "status output: '$output'"
  [ -z "$output" ]
}

# ================================================================
# 11. 再実行しても common 側 exclude の該当行は 1 行のまま
# ================================================================
@test "11: 同じtask-idで2回実行してもcommon側excludeの.worktree-guard-config行は1行" {
  TASK_ID="T-exclude-twice"
  run bash "$SCRIPT" --task-id "$TASK_ID"
  [ "$status" -eq 0 ]
  run bash "$SCRIPT" --task-id "$TASK_ID"
  [ "$status" -eq 0 ]
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  EXCLUDE=$(common_exclude "$WT_DIR")
  run grep -cxF ".worktree-guard-config" "$EXCLUDE"
  echo "count: '$output' in $EXCLUDE"
  [ "$output" = "1" ]
  run git -C "$WT_DIR" status --porcelain
  [ -z "$output" ]
}

# ================================================================
# 11b. 末尾改行のない既存 exclude の最終パターンを壊さない
# ================================================================
@test "11b: 末尾改行のない既存excludeに追記しても既存パターンが有効なまま" {
  TASK_ID="T-exclude-noeol"
  mkdir -p "$TEST_REPO/.git/info"
  printf 'user-pattern.tmp' > "$TEST_REPO/.git/info/exclude"
  touch "$TEST_REPO/user-pattern.tmp"
  run bash "$SCRIPT" --task-id "$TASK_ID"
  [ "$status" -eq 0 ]
  grep -qxF "user-pattern.tmp" "$TEST_REPO/.git/info/exclude"
  grep -qxF ".worktree-guard-config" "$TEST_REPO/.git/info/exclude"
  run git -C "$TEST_REPO" status --porcelain -- user-pattern.tmp
  [ -z "$output" ]
}

# ================================================================
# 12. global allowlist の glob (`.env.*`) が複数ファイルに一致しても全件コピーされる
#     (コメント行・空行・行末コメントは無視される)
# ================================================================
@test "12: global allowlistの.env.*で.env.aと.env.bの両方がコピーされる" {
  TASK_ID="T-glob"
  echo "A" > "$TEST_REPO/.env.a"
  echo "B" > "$TEST_REPO/.env.b"
  cat > "$HOME/.claude/worktree-copy.list" <<'LIST'
# comment line

.env.*   # trailing comment
LIST
  run bash "$SCRIPT" --task-id "$TASK_ID"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" != *"multiple patterns"* ]]
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  [ "$(cat "$WT_DIR/.env.a")" = "A" ]
  [ "$(cat "$WT_DIR/.env.b")" = "B" ]
}

# ================================================================
# 13. `@tag:x` 行は --skip-tag x で無視され、指定なしならコピーされる
# ================================================================
@test "13: @tag:x の行は--skip-tag xで無視される (指定なしならコピー)" {
  echo "A" > "$TEST_REPO/.env.a"
  echo "@tag:x .env.a" > "$HOME/.claude/worktree-copy.list"

  run bash "$SCRIPT" --task-id "T-tag-skip" --skip-tag x
  [ "$status" -eq 0 ]
  [ ! -e "$TEST_REPO/.claude/worktrees/agent-T-tag-skip/.env.a" ]

  # positive control: 同じ行が --skip-tag なしではコピーされる
  run bash "$SCRIPT" --task-id "T-tag-keep"
  [ "$status" -eq 0 ]
  [ -f "$TEST_REPO/.claude/worktrees/agent-T-tag-keep/.env.a" ]
}

# ================================================================
# 14. @junction の行き先がファイルなら WARN を出してその行だけ飛ばし、spawn は成功する
# ================================================================
@test "14: @junction先がworktree上でファイルならWARNで飛ばしexit 0" {
  TASK_ID="T-junction-file"
  # HEAD では d はファイル (worktree にはファイルとして checkout される)
  echo "tracked" > "$TEST_REPO/d"
  git -C "$TEST_REPO" add d
  git -C "$TEST_REPO" commit -q -m "add d"
  # main 側の作業ツリーでは d をディレクトリにする
  rm "$TEST_REPO/d"
  mkdir -p "$TEST_REPO/d"
  touch "$TEST_REPO/d/x"
  mkdir -p "$TEST_REPO/.claude"
  echo "@junction:d/" > "$TEST_REPO/.claude/worktree-copy.list"

  run bash "$SCRIPT" --task-id "$TASK_ID"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"cannot junction"* ]]
  WT_DIR="$TEST_REPO/.claude/worktrees/agent-${TASK_ID}"
  [ -f "$WT_DIR/d" ]
  echo "$output" | grep -q "^WORKTREE_ROOT="
}

# ================================================================
# 15. repo パスに空白を含んでも worktree が repo 配下に作られる
# ================================================================
@test "15: 空白を含むrepoパスでもworktreeがrepo配下に作られる" {
  SPACE_REPO="$BATS_TEST_TMPDIR/with space/repo"
  mkdir -p "$SPACE_REPO"
  git -C "$SPACE_REPO" init -q
  git -C "$SPACE_REPO" config user.email "test@example.com"
  git -C "$SPACE_REPO" config user.name "Test"
  git -C "$SPACE_REPO" commit -q --allow-empty -m "init"

  run bash -c "cd \"\$1\" && bash \"\$2\" --task-id T-space" _ "$SPACE_REPO" "$SCRIPT"
  echo "$output"
  [ "$status" -eq 0 ]
  [ -d "$SPACE_REPO/.claude/worktrees/agent-T-space" ]
  [ -f "$SPACE_REPO/.claude/worktrees/agent-T-space/.worktree-guard-config" ]
  [ ! -e "$BATS_TEST_TMPDIR/with/.claude" ]
  git -C "$SPACE_REPO" worktree remove --force "$SPACE_REPO/.claude/worktrees/agent-T-space"
}
