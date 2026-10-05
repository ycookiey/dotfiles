#!/usr/bin/env bash
# settings-template-reminder.sh — PreToolUse hook (Read|Edit|Write):
# ~/.claude*/settings.json を触る直前だけ、dotfiles テンプレートとのマージ型であることを Claude の文脈に差し込む。
# 止めない(permissionDecision を返さない)。
path=$(jq -r '.tool_input.file_path // ""')
# ホーム直下の .claude / .claude-* だけ。repo の .claude/settings.json(project 設定)は対象外
grep -Eq '[\\/](Users|home)[\\/][^\\/]+[\\/]\.claude(-[^\\/]+)?[\\/]settings\.json$' <<<"$path" || exit 0
cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"~/.claude*/settings.json は dotfiles/claude/settings.json をテンプレートとするマージ先。nushell/pwsh 起動ごとの dotcli sync がテンプレートのキー(model・hooks 等)を上書きし、テンプレートにないキーは残す(消さない)。テンプレートにあるキーを変える・全端末に効かせる変更はテンプレート側も編集する。手元や /model だけの変更は次の sync で戻る。詳細 ~/.claude/docs/symlinks.md"}}
JSON
