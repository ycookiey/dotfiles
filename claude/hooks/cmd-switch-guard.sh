#!/usr/bin/env bash
# cmd-switch-guard.sh — PreToolUse hook (Bash):
# Git Bash は単独の `/c` のような引数を Windows パス(`C:/`)に変えるため、
# `cmd.exe /c ...` は cmd が対話の画面で起動し、入力を待って止まる。
# その形のコマンドだけを止め、`//c` で書き直すよう返す。
# settings の `if` だけでは、複雑なコマンド(heredoc・ループ等)でも呼ばれるため、ここでも中身を見る。
command=$(jq -r '.tool_input.command // ""')
# 変換を止めた形は通す
grep -q 'MSYS2_ARG_CONV_EXCL' <<<"$command" && exit 0
grep -Eq '(^|[^[:alnum:]_./-])cmd([.]exe)?[[:space:]]+/[[:alpha:]]([[:space:]]|$)' <<<"$command" || exit 0
cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Git Bash は cmd の /c を C:/ に変えるので、cmd が対話の画面で起動して止まる。cmd.exe //c ... と書くか、MSYS2_ARG_CONV_EXCL='*' を付ける(ファイルを既定のアプリで開くなら PowerShell の Start-Process、フォルダなら explorer.exe も可)。詳細 ~/.claude/docs/bash.md の「MSYS パス変換」"}}
JSON
