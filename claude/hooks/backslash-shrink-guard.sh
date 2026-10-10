#!/usr/bin/env bash
# backslash-shrink-guard.sh — PreToolUse hook (Bash):
# Bash ツールは実行の前に `\\` を `\` 1 本に縮める。heredoc の本文と単一引用の中の `\\` は止めて
# Write ツールを使うよう返し、それ以外の `\\` は注意だけを差し込む(判定は backslash-shrink-guard.py)。
# 文書(~/.claude/docs/bash.md)があっても、python の heredoc に "\\r\\n" と書いて本物の改行を
# 入れた(2026-10-10)。
# `\\` が無いコマンドは python を起こさずに抜ける。hook の入力の JSON では `\\` は `\\\\` と書かれる。
input=$(cat)
[[ $input == *'\\\\'* ]] || exit 0
py=$(command -v python || command -v python3) || exit 0
printf '%s' "$input" | "$py" "$(dirname "$0")/backslash-shrink-guard.py"
