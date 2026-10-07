#!/usr/bin/env bash
# set-e-reminder.sh — PreToolUse hook (Bash):
# Bash ツールはコマンドを errexit の効かない文脈で動かすため、`set -e` を書いても
# 失敗の後に先へ進む(`( set -e; false; echo after )` でも after が出る)。
# `set -e` / `set -o errexit` を含むコマンドの直前だけ、その旨を Claude の文脈に差し込む。
# 止めない(permissionDecision を返さない = 通常の権限の流れ)。
command=$(jq -r '.tool_input.command // ""')
grep -Eq '(^|[^[:alnum:]_-])set[[:space:]]+(-[[:alpha:]]*e|-o[[:space:]]+errexit)' <<<"$command" || exit 0
cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Bash ツールでは set -e が効かない(errexit の効かない文脈で実行される。( set -e; false; echo after ) でも after が出る)。失敗で止めたい手順は && でつなぐか、各段の後で終了コードを確かめる。複数の検査を流して最後の結果だけを見ると、途中の失敗を見逃す。"}}
JSON
