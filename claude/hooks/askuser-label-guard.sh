#!/usr/bin/env bash
# askuser-label-guard.sh — PreToolUse hook (AskUserQuestion):
# ユーザーの UI では選択肢の description が表示されない(label・preview・直前の本文は表示される)。
# 選択肢の詳細は直前の本文か preview で説明し、label を A, B, C … の ID で始めて対応づける。
# label が ID で始まらない選択肢があれば止める。本文の説明の有無は確かめない
# (transcript に直前の本文が記録されないことがあり、確かめると説明済みでも止めてしまうため)。
set -uo pipefail
export LC_ALL=C.UTF-8

input=$(cat)

missing_id=()
while IFS= read -r label; do
  [ -z "$label" ] && continue
  grep -qE '^[A-Z][0-9]?([^A-Za-z0-9]|$)' <<<"$label" || missing_id+=("$label")
done < <(jq -r '.tool_input.questions[]?.options[]?.label // empty' <<<"$input" 2>/dev/null)

[ ${#missing_id[@]} -eq 0 ] && exit 0

reason="AskUserQuestion の選択肢の description はユーザーの UI に表示されない(label・preview・直前の本文は表示される)。各選択肢の中身・トレードオフは直前の本文か preview で説明し、label を A, B, C … の ID で始めて本文の説明と対応づけてから呼び直す。ID で始まらない label: $(printf '「%s」' "${missing_id[@]}")"
jq -nc --arg r "$reason" '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: $r}}'
