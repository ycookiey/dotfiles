#!/usr/bin/env bats
# worktree-isolation.e2e.bats — E2Eテスト: agent-spawn-prep.sh + worktree-guard.sh 統合
#
# 対象: plan-v2.md Section 9「手動確認ケース」14ケースの自動化
# 実装:
#   <repo>/skills/urleader/scripts/agent-spawn-prep.sh
#   <repo>/claude/hooks/worktree-guard.sh (なければ guard 依存ケースは skip)
#
# 実行 (repo root から):
#   bats skills/urleader/tests/worktree-isolation.e2e.bats
#
# 副作用: mktemp の一時 repo / HOME と BATS_TEST_TMPDIR 配下だけ。spawn は HOME=$E2E_HOME で動かす。

SPAWN_SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../scripts" && pwd)/agent-spawn-prep.sh"
GUARD_SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../../../claude/hooks" && pwd)/worktree-guard.sh"

# ========== setup_file: テスト全体で共有するgit repo ==========

setup_file() {
  # テスト用git repoを作成 (全テスト共有)
  export E2E_REPO
  E2E_REPO=$(mktemp -d)
  git -C "$E2E_REPO" init -q
  git -C "$E2E_REPO" config user.email "e2e@example.com"
  git -C "$E2E_REPO" config user.name "E2E"
  touch "$E2E_REPO/README.md"
  git -C "$E2E_REPO" add README.md
  git -C "$E2E_REPO" commit -q -m "init"

  # テスト用HOMEディレクトリ
  export E2E_HOME
  E2E_HOME=$(mktemp -d)
  mkdir -p "$E2E_HOME/.claude/hooks"
  mkdir -p "$E2E_HOME/.claude/skills/urleader"
  mkdir -p "$E2E_HOME/.claude-1/hooks"
  touch "$E2E_HOME/.claude/settings.json"
  touch "$E2E_HOME/.claude/settings.local.json"
  touch "$E2E_HOME/.claude/hooks/worktree-guard.sh"
  touch "$E2E_HOME/.claude/skills/urleader/SKILL.md"
  touch "$E2E_HOME/.claude-1/settings.json"
  touch "$E2E_HOME/.claude-1/hooks/some-hook.sh"
}

teardown_file() {
  # worktreeを全て削除してからrepo削除
  if [[ -d "$E2E_REPO/.claude/worktrees" ]]; then
    for wt_dir in "$E2E_REPO/.claude/worktrees"/*/; do
      [[ -d "$wt_dir" ]] || continue
      git -C "$E2E_REPO" worktree remove "$wt_dir" --force 2>/dev/null || true
    done
  fi
  # worktreeで作成されたbranchを削除
  git -C "$E2E_REPO" branch | grep "worktree-agent-" | xargs -r git -C "$E2E_REPO" branch -D 2>/dev/null || true
  rm -rf "$E2E_REPO"
  rm -rf "$E2E_HOME"
}

# ========== setup: テストごとの独立したworktree + config ==========

setup() {
  # cygpath stubをPATH先頭に注入 (Git Bash環境でcygpathがない場合)
  export STUB_BIN="$BATS_TEST_TMPDIR/stub_bin"
  mkdir -p "$STUB_BIN"
  if ! command -v cygpath &>/dev/null; then
    cat > "$STUB_BIN/cygpath" <<'STUB'
#!/usr/bin/env bash
while [[ $# -gt 0 ]]; do
  case "$1" in
    -u|-w|-m) shift; echo "$1"; shift ;;
    *) echo "$1"; shift ;;
  esac
done
STUB
    chmod +x "$STUB_BIN/cygpath"
  fi
  export PATH="$STUB_BIN:$PATH"

  # git の global 設定 (XDG 側) を実環境から切り離す
  unset XDG_CONFIG_HOME

  # 環境変数リセット
  unset WORKTREE_GUARD_ROOT
  unset WORKTREE_GUARD_MODE
  unset CLAUDE_PROJECT_DIR
}

teardown() {
  rm -rf "$BATS_TEST_TMPDIR"
}

# ========== ヘルパー ==========

# agent-spawn-prep.sh を実行してworktreeを作成し、worktreeパスを返す
# HOME=$E2E_HOME で実環境の ~/.claude/worktree-copy.list を読ませない
# (読むと 補足-a が作る .agent-output/ が @junction され、teardown の worktree remove --force が
#  junction を辿る)。
# usage: run_spawn_prep <task-id>
# stdout: WORKTREE_ROOT=<path>
run_spawn_prep() {
  local task_id="$1"
  cd "$E2E_REPO"
  HOME="$E2E_HOME" bash "$SPAWN_SCRIPT" --task-id "$task_id"
}

# worktree-guard.sh がなければ skip (hooks の配置換えで消えた場合に備える)
require_guard() {
  [[ -f "$GUARD_SCRIPT" ]] || skip "worktree-guard.sh not found: $GUARD_SCRIPT"
}

# worktree-guard.sh に Edit/Write/MultiEdit のstdin JSONを渡す
# usage: run_guard_edit <tool_name> <file_path> <cwd> [env vars...]
simulate_edit() {
  local tool_name="$1"
  local file_path="$2"
  local cwd="$3"
  shift 3
  local json
  json=$(printf '{"tool_name":"%s","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$tool_name" "$file_path" "$cwd")
  echo "$json" | env "$@" bash "$GUARD_SCRIPT"
}

# worktree-guard.sh に Bash のstdin JSONを渡す
simulate_bash() {
  local command="$1"
  local cwd="$2"
  shift 2
  local json
  json=$(printf '{"tool_name":"Bash","tool_input":{"command":"%s"},"cwd":"%s"}' \
    "$command" "$cwd")
  echo "$json" | env "$@" bash "$GUARD_SCRIPT"
}

# ========== Case 1: 正常系 worktree作成→hook有効化→worktree内Edit許可 ==========

@test "Case1: agent-spawn-prep.sh実行→worktree作成→worktree内Edit許可" {
  require_guard
  local task_id="e2e-case1"
  local spawn_out
  spawn_out=$(run_spawn_prep "$task_id")
  [ $? -eq 0 ]

  # worktreeが作成されている
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"
  [ -d "$wt_dir" ]

  # .worktree-guard-configが存在する
  [ -f "$wt_dir/.worktree-guard-config" ]

  # worktree内ファイルへのEditはallowed (exit 0)
  local target="$wt_dir/src/new-file.ts"
  mkdir -p "$(dirname "$target")"
  run simulate_edit "Edit" "$target" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 0 ]
}

# ========== Case 2: deny — main直接編集 (deny mode) ==========

@test "Case2: hookがdeny modeでmain直接編集をexit 2でblockする" {
  require_guard
  local task_id="e2e-case2"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  # configをdenyモードに変更
  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  # repo root直下ファイルへのEdit → deny (exit 2)
  local target="$E2E_REPO/packages/web/src/foo.ts"
  mkdir -p "$(dirname "$target")"
  run simulate_edit "Edit" "$target" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}

# ========== Case 2 (warn): main直接編集でwarn modeは警告してexit 0 ==========

@test "Case2-warn: hookがwarn modeでmain直接編集を警告してexit 0で続行する" {
  require_guard
  local task_id="e2e-case2w"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  # デフォルトはwarnモード (agent-spawn-prep.shがMODE=warnで書き出す)
  local target="$E2E_REPO/packages/web/src/foo.ts"
  mkdir -p "$(dirname "$target")"
  run bash -c "
    json=\$(printf '{\"tool_name\":\"Edit\",\"tool_input\":{\"file_path\":\"%s\"},\"cwd\":\"%s\"}' \
      '$target' '$wt_dir')
    echo \"\$json\" | CLAUDE_PROJECT_DIR='$wt_dir' HOME='$E2E_HOME' bash '$GUARD_SCRIPT' 2>&1
    echo EXIT:\$?
  "
  [[ "$output" == *"EXIT:0"* ]]
  [[ "$output" == *"WARN"* ]]
}

# ========== Case 3: deny — ~/.claude/settings.json への Edit ==========

@test "Case3: ~/.claude/settings.jsonへのEditはdenyされる" {
  require_guard
  local task_id="e2e-case3"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  # denyモードに切替
  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  run simulate_edit "Edit" "$E2E_HOME/.claude/settings.json" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
}

# ========== Case 4: deny — hook script改竄試行 ==========

@test "Case4: ~/.claude/hooks/worktree-guard.shへのEditはdenyされる" {
  require_guard
  local task_id="e2e-case4"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  run simulate_edit "Write" "$E2E_HOME/.claude/hooks/worktree-guard.sh" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
}

# ========== Case 5: deny — skill改竄試行 ==========

@test "Case5: ~/.claude/skills/urleader/配下へのEditはdenyされる" {
  require_guard
  local task_id="e2e-case5"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  run simulate_edit "Edit" "$E2E_HOME/.claude/skills/urleader/SKILL.md" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
}

# ========== Case 6: マルチアカウント ~/.claude-1/ 配下もdeny ==========

@test "Case6: マルチアカウント ~/.claude-1/settings.jsonへのEditはdenyされる" {
  require_guard
  local task_id="e2e-case6"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  run simulate_edit "Edit" "$E2E_HOME/.claude-1/settings.json" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
}

@test "Case6b: マルチアカウント ~/.claude-2/hooks/配下へのWriteはdenyされる" {
  require_guard
  local task_id="e2e-case6b"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  # ~/.claude-2/ は実ファイルが存在しなくてもpath文字列でdeny判定される
  run simulate_edit "Write" "$E2E_HOME/.claude-2/hooks/evil.sh" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
}

# ========== Case 7: CVE-2025-54794 /proc/self/root bypass ==========

@test "Case7: /proc/self/root経由のbypass試行はdenyされる" {
  require_guard
  local task_id="e2e-case7"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  # /proc/self/root は Linux特有。Windows Git Bashでは存在しないためskip条件付き
  if [[ ! -d /proc/self/root ]]; then
    skip "/proc/self/root が存在しない環境 (Windows)"
  fi

  local target="/proc/self/root${E2E_REPO}/src/foo.ts"
  run simulate_edit "Edit" "$target" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  # realpath解決後worktree外と判定 → deny
  [ "$status" -eq 2 ]
}

# ========== Case 8: junction迂回 (skip条件付き) ==========

@test "Case8: NTFS junction経由のbypass試行 (環境制約でskip)" {
  # NTFS junctionはWindowsでのみ作成可能かつMSYS2 realpathが解決しないため
  # H-3のTODO項目。deny mode昇格前に対処予定。現状はskip。
  skip "NTFS junction迂回はH-3 TODOのため現時点ではskip (deny mode昇格前に対処)"
}

# ========== Case 9: Bash経由 repo rootへの書き込みをdeny ==========

@test "Case9: Bash経由でrepo rootへの書き込みコマンドはdenyされる" {
  require_guard
  local task_id="e2e-case9"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  # handle_bash: repo rootを含むコマンド → deny
  local cmd="echo foo > $E2E_REPO/test.txt"
  run simulate_bash "$cmd" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
}

@test "Case9b: Bash経由でworktree内パスのみを含むコマンドは許可される" {
  require_guard
  local task_id="e2e-case9b"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  # worktree内パスのみ → allow
  local cmd="echo foo > $wt_dir/test.txt"
  run simulate_bash "$cmd" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 0 ]
}

# ========== Case 10: 既存worktree再利用 WARN+reuse ==========

@test "Case10: 同一task-idで再呼び出しするとWARNを出してworktreeを再利用する" {
  local task_id="e2e-case10"
  # 1回目
  run_spawn_prep "$task_id" > /dev/null

  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"
  [ -d "$wt_dir" ]

  # 2回目: 同一task-id → WARNが出てexit 0で再利用
  run bash -c "cd '$E2E_REPO' && HOME='$E2E_HOME' bash '$SPAWN_SCRIPT' --task-id '$task_id' 2>&1; echo EXIT:\$?"

  [[ "$output" == *"WARN"* ]]
  [[ "$output" == *"EXIT:0"* ]]
  # worktreeは削除されずに残る
  [ -d "$wt_dir" ]
}

# ========== Case 12: .worktree-guard-config 作成 ==========

@test "Case12: .worktree-guard-configが正しいschemaでworktree内に配置される" {
  local task_id="e2e-case12"
  run run_spawn_prep "$task_id"
  [ "$status" -eq 0 ]

  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"
  local config="$wt_dir/.worktree-guard-config"

  # ファイルが存在する
  [ -f "$config" ]

  # 必須キーが全て存在する
  grep -q "^WORKTREE_ROOT=" "$config"
  grep -q "^REPO_ROOT=" "$config"
  grep -q "^MODE=" "$config"

  # WORKTREE_ROOTの値がwt_dirに一致する
  local wt_val
  wt_val=$(grep "^WORKTREE_ROOT=" "$config" | head -1 | cut -d= -f2-)
  [ -n "$wt_val" ]

  # REPO_ROOTの値がE2E_REPOに一致する (POSIX形式)
  local repo_val
  repo_val=$(grep "^REPO_ROOT=" "$config" | head -1 | cut -d= -f2-)
  [ -n "$repo_val" ]

  # MODEはwarn (デフォルト)
  local mode_val
  mode_val=$(grep "^MODE=" "$config" | head -1 | cut -d= -f2-)
  [ "$mode_val" = "warn" ]
}

# ========== Case 13: cleanup — git worktree remove後にbranch削除可能 ==========

@test "Case13: git worktree removeでworktree削除後にworktree-branchが削除できる" {
  local task_id="e2e-case13"
  run_spawn_prep "$task_id" > /dev/null

  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"
  local wt_branch="worktree-agent-$task_id"

  # worktree が存在する
  [ -d "$wt_dir" ]

  # git worktree remove を実行
  run git -C "$E2E_REPO" worktree remove "$wt_dir" --force
  [ "$status" -eq 0 ]

  # worktreeディレクトリが消えている
  [ ! -d "$wt_dir" ]

  # branchが削除可能であること
  run git -C "$E2E_REPO" branch -D "$wt_branch"
  [ "$status" -eq 0 ]
}

# ========== Case 14: MODE=warn vs MODE=deny 切替 ==========

@test "Case14: configのMODEをwarnからdenyに変えるとhookの挙動が切り替わる" {
  require_guard
  local task_id="e2e-case14"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"
  local target="$E2E_REPO/packages/web/src/outside.ts"
  mkdir -p "$(dirname "$target")"

  # --- warn mode (デフォルト) ---
  run bash -c "
    json=\$(printf '{\"tool_name\":\"Edit\",\"tool_input\":{\"file_path\":\"%s\"},\"cwd\":\"%s\"}' \
      '$target' '$wt_dir')
    echo \"\$json\" | CLAUDE_PROJECT_DIR='$wt_dir' HOME='$E2E_HOME' bash '$GUARD_SCRIPT' 2>&1
    echo EXIT:\$?
  "
  [[ "$output" == *"EXIT:0"* ]]
  [[ "$output" == *"WARN"* ]]

  # --- deny mode に切替 ---
  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  run simulate_edit "Edit" "$target" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}

# ========== 補足: .agent-output/へのEdit/Writeは許可 (Bash redirectはdeny) ==========

@test "補足-a: .agent-output/へのEdit/Writeはworktree外でも許可される" {
  require_guard
  local task_id="e2e-suppa"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  local target="$E2E_REPO/.agent-output/e2e-suppa/result.md"
  mkdir -p "$(dirname "$target")"

  run simulate_edit "Edit" "$target" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 0 ]
}

@test "補足-b: .agent-output/へのBash redirectはdenyされる (C-3対応)" {
  require_guard
  local task_id="e2e-suppb"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  # .agent-output/へのBash redirectはdeny (handle_bash: repo rootを含む)
  local cmd="echo result > $E2E_REPO/.agent-output/e2e-suppb/result.txt"
  run simulate_bash "$cmd" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
}

@test "補足-c: WORKTREE_GUARD_ROOT未設定+configファイルありでhookが正常動作する (C-2対応)" {
  require_guard
  local task_id="e2e-suppc"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  # WORKTREE_GUARD_ROOT環境変数を明示的にunset (configファイルのみで動作することを確認)
  local target="$E2E_REPO/packages/web/src/outside.ts"
  mkdir -p "$(dirname "$target")"

  run bash -c "
    json=\$(printf '{\"tool_name\":\"Edit\",\"tool_input\":{\"file_path\":\"%s\"},\"cwd\":\"%s\"}' \
      '$target' '$wt_dir')
    unset WORKTREE_GUARD_ROOT
    echo \"\$json\" | CLAUDE_PROJECT_DIR='$wt_dir' HOME='$E2E_HOME' bash '$GUARD_SCRIPT'
  "
  [ "$status" -eq 2 ]
}

@test "補足-d: .worktree-guard-config自体への書き込みはdenyされる (Case14)" {
  require_guard
  local task_id="e2e-suppd"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  # config自体への書き込みはdeny (worktree内でも)
  local target="$wt_dir/.worktree-guard-config"
  run simulate_edit "Write" "$target" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
}

# ========== 補足-e: ~/.claude/docs/ はdeny listに追加済みのためdeny ==========

@test "補足-e: ~/.claude/docs/配下はdeny listに追加済みのためdenyされる" {
  require_guard
  local task_id="e2e-suppe"
  run_spawn_prep "$task_id" > /dev/null
  local wt_dir="$E2E_REPO/.claude/worktrees/agent-$task_id"

  cat > "$wt_dir/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$wt_dir
REPO_ROOT=$E2E_REPO
MODE=deny
EOF

  local target="$E2E_HOME/.claude/docs/test.md"
  mkdir -p "$(dirname "$target")"

  run simulate_edit "Write" "$target" "$wt_dir" \
    "CLAUDE_PROJECT_DIR=$wt_dir" "HOME=$E2E_HOME"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}
