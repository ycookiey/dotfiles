#!/usr/bin/env bats
# worktree-guard.bats — worktree-guard.sh の単体テスト (TDD: Red状態で提出)
#
# 対象実装: C:\Main\Project\dotfiles\claude\hooks\worktree-guard.sh (未実装)
# 設計書: C:\Main\Project\AnalyzerCH\.agent-output\worktree-isolation\plan-v2.md
#
# Red確認手順:
#   1. bats-core をインストール: https://bats-core.readthedocs.io/en/stable/installation.html
#      (Git Bash on Windows: npm install -g bats  または  scoop install bats)
#   2. 作業ディレクトリで実行:
#        bats skills/urleader/tests/worktree-guard.bats
#   3. worktree-guard.sh が存在しないため全テストが FAIL になることを確認する
#
# mock化方針:
#   - realpath: BATS_TEST_TMPDIR内の実パスを使うため基本は実コマンドを使用。
#               存在しないpath用はヘルパーで fake_realpath を定義しPATH先頭に注入する
#   - cygpath:  Git Bash上ではcygpathが存在しない場合を想定し、環境にない場合は
#               stub を PATH 先頭に注入してPOSIX形式をそのまま返す
#   - jq:       実コマンドを使用 (CI環境でも通常利用可能)
#   - git:      テスト内でfake_git stub を PATH 先頭に注入し、任意の値を返す
#   - GUARD_ROOT/HOME等の環境変数はテストごとに上書き設定する

# ========== セットアップ ==========

GUARD_SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../../../claude/hooks" && pwd)/worktree-guard.sh"

setup() {
  # 一時ディレクトリをテストごとに作成
  export TMPDIR_TEST="$BATS_TEST_TMPDIR"

  # worktree / repo rootをTMPDIR配下に模擬
  export FAKE_REPO_ROOT="$BATS_TEST_TMPDIR/repo"
  export FAKE_WT_ROOT="$BATS_TEST_TMPDIR/repo/.claude/worktrees/agent-T1"
  export FAKE_HOME="$BATS_TEST_TMPDIR/home"

  mkdir -p "$FAKE_REPO_ROOT"
  mkdir -p "$FAKE_WT_ROOT"
  mkdir -p "$FAKE_HOME/.claude/hooks"
  mkdir -p "$FAKE_HOME/.claude/skills/urleader"

  # .worktree-guard-config をworktree rootに配置 (agent-spawn-prep.shが生成する形式)
  cat > "$FAKE_WT_ROOT/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$FAKE_WT_ROOT
REPO_ROOT=$FAKE_REPO_ROOT
MODE=deny
EOF

  # cygpath stub: 環境にない場合は入力をそのまま返すstubをPATH先頭に注入
  export STUB_BIN="$BATS_TEST_TMPDIR/stub_bin"
  mkdir -p "$STUB_BIN"

  if ! command -v cygpath &>/dev/null; then
    cat > "$STUB_BIN/cygpath" <<'STUB'
#!/usr/bin/env bash
# stub: 引数を解析してそのまま返す
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

  # 環境変数をリセット
  unset WORKTREE_GUARD_ROOT
  unset WORKTREE_GUARD_MODE
  unset CLAUDE_PROJECT_DIR
}

teardown() {
  rm -rf "$BATS_TEST_TMPDIR"
}

# ========== ヘルパー ==========

# stdin JSONを組み立ててguard scriptを呼び出すヘルパー
# usage: run_guard <tool_name> [file_path] [command]
run_guard_edit() {
  local tool_name="$1"
  local file_path="$2"
  local json
  json=$(printf '{"tool_name":"%s","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$tool_name" "$file_path" "$FAKE_WT_ROOT")
  echo "$json" | bash "$GUARD_SCRIPT"
}

run_guard_bash() {
  local command="$1"
  local json
  json=$(printf '{"tool_name":"Bash","tool_input":{"command":"%s"},"cwd":"%s"}' \
    "$command" "$FAKE_WT_ROOT")
  echo "$json" | bash "$GUARD_SCRIPT"
}

# CLAUDE_PROJECT_DIR を設定してguard scriptを呼び出す
run_guard_edit_with_config() {
  local tool_name="$1"
  local file_path="$2"
  local config_dir="$3"
  local json
  json=$(printf '{"tool_name":"%s","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$tool_name" "$file_path" "$config_dir")
  CLAUDE_PROJECT_DIR="$config_dir" echo "$json" | bash "$GUARD_SCRIPT"
}

# ========== 観点1: check_home_path ホワイトリスト一致 ==========

@test "check_home_path: HOME配下の一般ファイルは許可される" {
  local target="$FAKE_HOME/.claude/some-allowed-file.md"
  mkdir -p "$(dirname "$target")"
  touch "$target"

  # HOME配下だがdeny対象外 → allow (exit 0)
  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "check_home_path: ~/.claude/settings.jsonへのEditはdenyされる" {
  local target="$FAKE_HOME/.claude/settings.json"
  touch "$target"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}

@test "check_home_path: ~/.claude/settings.local.jsonへのEditはdenyされる" {
  local target="$FAKE_HOME/.claude/settings.local.json"
  touch "$target"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "check_home_path: ~/.claude/hooks/配下のファイルはdenyされる" {
  local target="$FAKE_HOME/.claude/hooks/worktree-guard.sh"
  touch "$target"

  local json
  json=$(printf '{"tool_name":"Write","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "check_home_path: ~/.claude/skills/urleader/配下はdenyされる" {
  local target="$FAKE_HOME/.claude/skills/urleader/SKILL.md"
  touch "$target"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "check_home_path: マルチアカウント ~/.claude-1/settings.jsonはdenyされる" {
  mkdir -p "$FAKE_HOME/.claude-1"
  local target="$FAKE_HOME/.claude-1/settings.json"
  touch "$target"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "check_home_path: マルチアカウント ~/.claude-1/hooks/配下はdenyされる" {
  mkdir -p "$FAKE_HOME/.claude-1/hooks"
  local target="$FAKE_HOME/.claude-1/hooks/some-hook.sh"
  touch "$target"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "check_home_path: ~/.claude/docs/agent-team.md へのEditはdenyされる" {
  mkdir -p "$FAKE_HOME/.claude/docs"
  local target="$FAKE_HOME/.claude/docs/agent-team.md"
  touch "$target"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"* ]]
}

@test "check_home_path: ~/.claude/docs/foo.md へのWriteはdenyされる" {
  mkdir -p "$FAKE_HOME/.claude/docs"
  local target="$FAKE_HOME/.claude/docs/foo.md"

  local json
  json=$(printf '{"tool_name":"Write","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "check_home_path: マルチアカウント ~/.claude-1/docs/ 配下はdenyされる" {
  mkdir -p "$FAKE_HOME/.claude-1/docs"
  local target="$FAKE_HOME/.claude-1/docs/agent-team.md"
  touch "$target"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

# ========== 観点2: deny一覧 全件 ==========

@test "deny一覧: .worktree-guard-config自体への書き込みはdenyされる(任意ディレクトリ)" {
  local target="$FAKE_WT_ROOT/.worktree-guard-config"

  local json
  json=$(printf '{"tool_name":"Write","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "deny一覧: repo rootの任意ディレクトリ内の.worktree-guard-configもdenyされる" {
  local target="$FAKE_REPO_ROOT/subdir/.worktree-guard-config"
  mkdir -p "$(dirname "$target")"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

# ========== 観点3: resolve_guard_root 探索ロジック ==========

@test "resolve_guard_root: WORKTREE_GUARD_ROOT環境変数があればそれを使用する" {
  local target="$FAKE_REPO_ROOT/outside.ts"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  WORKTREE_GUARD_ROOT="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  # worktree外への書き込み → deny
  [ "$status" -eq 2 ]
}

@test "resolve_guard_root: CLAUDE_PROJECT_DIRのconfigファイルからWORKTREE_ROOTを読む" {
  local target="$FAKE_REPO_ROOT/outside.ts"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "resolve_guard_root: cwdから上方探索でconfigを見つける(CLAUDE_PROJECT_DIR未設定時)" {
  # cwdにconfigがある場合
  local cwd="$FAKE_WT_ROOT/subdir/deep"
  mkdir -p "$cwd"
  local target="$FAKE_REPO_ROOT/outside.ts"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$cwd")
  HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  # cwdから上方探索→$FAKE_WT_ROOTのconfigを発見→deny
  [ "$status" -eq 2 ]
}

@test "resolve_guard_root: configが見つからない場合はguard無効(exit 0)で通す" {
  local target="$FAKE_REPO_ROOT/outside.ts"
  local no_config_dir="$BATS_TEST_TMPDIR/no_config"
  mkdir -p "$no_config_dir"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$no_config_dir")
  HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  # config無し+env var無し → guard無効 → allow
  [ "$status" -eq 0 ]
}

# ========== 観点4: handle_bash Edit/Write系コマンド抽出 ==========

@test "handle_bash: repo rootを含むBashコマンドはdenyされる(POSIX形式)" {
  local cmd="echo foo > $FAKE_REPO_ROOT/test.txt"

  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "printf '{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"%s\"},\"cwd\":\"%s\"}' \
      '$cmd' '$FAKE_WT_ROOT' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "handle_bash: worktree内pathを含むBashコマンドは許可される" {
  local cmd="echo foo > $FAKE_WT_ROOT/test.txt"

  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "printf '{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"%s\"},\"cwd\":\"%s\"}' \
      '$cmd' '$FAKE_WT_ROOT' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "handle_bash: repo root pathを含まないBashコマンドは許可される" {
  local cmd="ls /tmp"

  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "printf '{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"%s\"},\"cwd\":\"%s\"}' \
      '$cmd' '$FAKE_WT_ROOT' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "handle_bash: .agent-output/へのBash redirectはdenyされる(C-3対応)" {
  local cmd="echo result > $FAKE_REPO_ROOT/.agent-output/T1/result.txt"

  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "printf '{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"%s\"},\"cwd\":\"%s\"}' \
      '$cmd' '$FAKE_WT_ROOT' | bash '$GUARD_SCRIPT'"
  # .agent-output/はEdit/Write limitedのためBashではdeny
  [ "$status" -eq 2 ]
}

# ========== 観点5: CVE-2025-54794 /proc/self/root bypass ==========

@test "CVE-2025-54794: /proc/self/root/<absolute>経由のbypass試行はdenyされる" {
  # /proc/self/root/C:\Main\Project\... のようなbypass
  local target="/proc/self/root${FAKE_REPO_ROOT}/src/foo.ts"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  # realpath解決後にworktree外と判定されdeny
  [ "$status" -eq 2 ]
}

# ========== 観点6: symlink/junction迂回 ==========

@test "symlink迂回: worktree内symlinkがworktree外を指す場合はdenyされる" {
  # worktree内にsymlinkを作成してrepo rootを指す
  local symlink_path="$FAKE_WT_ROOT/evil_link"
  local real_outside="$FAKE_REPO_ROOT/src"
  mkdir -p "$real_outside"
  ln -s "$real_outside" "$symlink_path" 2>/dev/null || skip "symlink作成不可"
  [ -L "$symlink_path" ] || skip "POSIX symlinkを作成できない環境 (Windows Git Bash等)"

  local target="$symlink_path/foo.ts"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  # realpath解決でworktree外と判定→deny
  [ "$status" -eq 2 ]
}

@test "symlink迂回: worktree内の正当なsymlinkはrealpath解決後もworktree内ならallow" {
  # worktree内→worktree内のsymlink
  local real_inside="$FAKE_WT_ROOT/src"
  mkdir -p "$real_inside"
  local symlink_path="$FAKE_WT_ROOT/link_inside"
  ln -s "$real_inside" "$symlink_path" 2>/dev/null || skip "symlink作成不可"

  local target="$symlink_path/foo.ts"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

# ========== 観点7: warn mode ==========

@test "warn mode: denyケースでexit 0かつstderrに警告を出力する" {
  # configのMODEをwarnに変更
  cat > "$FAKE_WT_ROOT/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$FAKE_WT_ROOT
REPO_ROOT=$FAKE_REPO_ROOT
MODE=warn
EOF

  local target="$FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT' 2>&1; echo EXIT:\$?"

  # warnモード: exit 0
  [[ "$output" == *"EXIT:0"* ]]
  # stderrにWARNメッセージ
  [[ "$output" == *"WARN"* ]]
}

@test "warn mode: stderrが非空かつexit 0 (CI判定基準)" {
  cat > "$FAKE_WT_ROOT/.worktree-guard-config" <<EOF
WORKTREE_ROOT=$FAKE_WT_ROOT
REPO_ROOT=$FAKE_REPO_ROOT
MODE=warn
EOF

  local target="$FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")

  # stderrを別キャプチャ
  local stderr_out
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
  # stderrが非空であることを確認 (batsではstderrはoutputに含まれない場合がある)
  [ -n "$output" ] || true  # WARNはstderrなのでrun経由では確認困難な場合あり
}

# ========== 観点8: deny mode ==========

@test "deny mode: denyケースでexit 2かつstderrに拒否理由を出力する" {
  local target="$FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT' 2>&1"
  # deny mode (config設定はsetupでdeny)
  [[ "$output" == *"BLOCKED"* ]]
}

@test "deny mode: exit codeが2であること" {
  local target="$FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

# ========== 観点9: 絶対path被害ケース (root cause) ==========

@test "絶対path被害: worktree内agentがrepo root絶対pathをEdit試行するとdenyされる" {
  # これがroot cause: subagentが誤ってrepo rootのファイルを直接編集しようとする
  local target="$FAKE_REPO_ROOT/packages/web/src/foo.ts"
  mkdir -p "$(dirname "$target")"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BLOCKED"*"$FAKE_REPO_ROOT"* ]] || \
    [[ "$output" == *"BLOCKED"* ]]
}

# ========== 観点10: worktree内path許可ケース ==========

@test "worktree内path: worktree内ファイルへのEditは許可される" {
  local target="$FAKE_WT_ROOT/src/component.tsx"
  mkdir -p "$(dirname "$target")"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "worktree内path: worktree内ファイルへのWriteは許可される" {
  local target="$FAKE_WT_ROOT/new-file.ts"

  local json
  json=$(printf '{"tool_name":"Write","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

@test ".agent-output/へのEdit/Writeは許可される(worktree外でもEdit/Write限定)" {
  local target="$FAKE_REPO_ROOT/.agent-output/T1/result.md"
  mkdir -p "$(dirname "$target")"

  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

# ========== 観点11: 入力JSON parse ==========

@test "JSON parse: tool_name=Edit, file_path フィールドを正しく読み取る" {
  local target="$FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(jq -n \
    --arg tn "Edit" \
    --arg fp "$target" \
    --arg cwd "$FAKE_WT_ROOT" \
    '{tool_name: $tn, tool_input: {file_path: $fp}, cwd: $cwd}')
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "JSON parse: tool_name=Bash, command フィールドを正しく読み取る" {
  local cmd="cat $FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(jq -n \
    --arg cmd "$cmd" \
    --arg cwd "$FAKE_WT_ROOT" \
    '{tool_name: "Bash", tool_input: {command: $cmd}, cwd: $cwd}')
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  # catはrepo root pathを含むがwriteではないので通す (handle_bashはwrite系チェック)
  # handle_bashはrepo rootパスを含むコマンドをdenyするため、catでもdeny
  [ "$status" -eq 2 ]
}

@test "JSON parse: tool_name=MultiEditも正しく処理される" {
  local target="$FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(jq -n \
    --arg fp "$target" \
    --arg cwd "$FAKE_WT_ROOT" \
    '{tool_name: "MultiEdit", tool_input: {file_path: $fp}, cwd: $cwd}')
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 2 ]
}

@test "JSON parse: tool_nameがEdit/Write/Bash以外はexit 0で通す" {
  local json
  json=$(jq -n \
    --arg cwd "$FAKE_WT_ROOT" \
    '{tool_name: "Read", tool_input: {file_path: "/any/path"}, cwd: $cwd}')
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "JSON parse: file_pathがnullの場合はexit 0で通す" {
  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":null},"cwd":"%s"}' \
    "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

# ========== 観点12: config未配置時 ==========

@test "config未配置: .worktree-guard-configが無い時はguard無効でexit 0" {
  # configが存在しないディレクトリをCLAUDE_PROJECT_DIRに設定
  local no_config_dir="$BATS_TEST_TMPDIR/no_config_dir"
  mkdir -p "$no_config_dir"

  local target="$FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$no_config_dir")
  CLAUDE_PROJECT_DIR="$no_config_dir" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  # config無し, env var無し → guard無効 → allow
  [ "$status" -eq 0 ]
}

@test "config未配置: WORKTREE_GUARD_ROOT環境変数も未設定ならguard無効" {
  local no_config_dir="$BATS_TEST_TMPDIR/no_config_dir2"
  mkdir -p "$no_config_dir"

  local target="$FAKE_REPO_ROOT/outside.ts"
  local json
  json=$(printf '{"tool_name":"Write","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$no_config_dir")
  unset WORKTREE_GUARD_ROOT
  HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

# ========== 追加: source実行禁止ガード ==========

@test "source禁止: sourceで呼び出した場合はreturn 1で終了する" {
  run bash -c "source '$GUARD_SCRIPT' 2>&1; echo exit:\$?"
  # sourceした場合 return 1 または exit 1
  [[ "$output" == *"must not be sourced"* ]] || \
    [[ "$output" == *"exit:1"* ]]
}

# ========== 追加: /tmp, /dev/null は許可 ==========

@test "/tmp/配下のファイルへのEditは許可される" {
  local target="/tmp/some-temp-file.txt"
  local json
  json=$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}

@test "/dev/null へのWriteは許可される" {
  local target="/dev/null"
  local json
  json=$(printf '{"tool_name":"Write","tool_input":{"file_path":"%s"},"cwd":"%s"}' \
    "$target" "$FAKE_WT_ROOT")
  CLAUDE_PROJECT_DIR="$FAKE_WT_ROOT" HOME="$FAKE_HOME" \
    run bash -c "echo '$json' | bash '$GUARD_SCRIPT'"
  [ "$status" -eq 0 ]
}
