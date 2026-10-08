#!/usr/bin/env bash
# post-push-reminder.sh — PostToolUse hook (Bash):
# `git push` が成功した直後だけ、その repo の push 後の作業(CI の見張り・画面の
# 流れの一覧)を、push した commit の名前つきで Claude の文脈に差し込む。
# push 前の hook(push-check-reminder.sh)の文は push の前に出るため、push が
# 済んだ時点では効かず、一覧の作成を流し忘れた(2026-10-08)。止めはしない。
input=$(cat)
command=$(jq -r '.tool_input.command // ""' <<<"$input")
grep -Eq '(^|[^[:alnum:]_-])git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-[:space:]][^[:space:]]*)?)*[[:space:]]+push([[:space:]]|$)' <<<"$command" || exit 0

# Bash の結果の形は版で違いうるので、文字の入る欄をまとめて読む。
output=$(jq -r '.tool_response
  | if type == "object" then ([.stdout, .stderr, .output] | map(select(type == "string")) | join("\n"))
    else tostring end' <<<"$input")
# 成功した push だけが `old..new  src -> dst` の行を出す。
commit=$(grep -Eo '[0-9a-f]{7,40}\.\.\.?[0-9a-f]{7,40}[[:space:]]+[^[:space:]]+[[:space:]]+->' <<<"$output" \
  | tail -n 1 | sed -E 's/^[0-9a-f]+\.\.\.?([0-9a-f]+).*/\1/')
[ -n "$commit" ] || exit 0

# push した repo: command の中の最後の `cd <dir>` か `git -C <dir>`、なければ hook の cwd。
dir=$(grep -Eo '(^|[;&|[:space:]])cd[[:space:]]+[^;&|[:space:]]+' <<<"$command" | tail -n 1 | sed -E 's/^.*cd[[:space:]]+//')
[ -n "$dir" ] || dir=$(grep -Eo 'git[[:space:]]+-C[[:space:]]+[^[:space:]]+' <<<"$command" | tail -n 1 | sed -E 's/^git[[:space:]]+-C[[:space:]]+//')
[ -n "$dir" ] || dir=$(jq -r '.cwd // ""' <<<"$input")
dir=${dir%\"}
dir=${dir#\"}

scripts=()
[ -f "$dir/scripts/watch-push.sh" ] && scripts+=("scripts/watch-push.sh $commit")
[ -f "$dir/scripts/flows-publish.sh" ] && scripts+=("scripts/flows-publish.sh $commit")
[ ${#scripts[@]} -gt 0 ] || exit 0

list=$(printf '「%s」' "${scripts[@]}")
jq -nc --arg c "push 済み($commit、$dir)。この repo の push 後の作業 ${list} を、今それぞれ run_in_background で起動する。片方だけで済ませない。" \
  '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $c}}'
