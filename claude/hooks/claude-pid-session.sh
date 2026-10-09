#!/bin/bash
# SessionStart hook: Claude Code のプロセス ID (CLAUDE_PID) ごとに、hook の入力 JSON
# (session_id / transcript_path / cwd / source) を保存する。
# 端末側のツール (WezTerm のセッション復元) が、動いている claude から会話 ID を引くために使う。
# /clear や /resume でも SessionStart が発火するので、同じ pid のファイルが最新の会話に上書きされる。
# 失敗しても Claude Code の起動は止めない。
dir="$HOME/.local/state/claude-pid-sessions"
if [ -z "${CLAUDE_PID:-}" ] || ! mkdir -p "$dir" 2>/dev/null; then
  cat >/dev/null
  exit 0
fi
tmp="$dir/$CLAUDE_PID.json.tmp"
cat >"$tmp" && mv -f "$tmp" "$dir/$CLAUDE_PID.json"
exit 0
