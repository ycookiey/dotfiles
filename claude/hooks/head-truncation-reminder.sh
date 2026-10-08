#!/usr/bin/env bash
# head-truncation-reminder.sh — PostToolUse hook (Bash):
# command の最後の `| head` の上限まで出力が届いていたら(= 切られた行がありうる)、
# 「ほかにない」と結論する前に件数を数えるよう Claude の文脈に差し込む。
# grep の出力を head で切り、切られた行にあった使用箇所を見落とした(2026-10-07)。
# 上限に届かなかったとき(全部出た)は何も出さない。止めはしない。
input=$(cat)
command=$(jq -r '.tool_input.command // ""' <<<"$input")
# 最後のパイプの head と、その行数(-n N・-N・省略時は 10)。
last=$(grep -Eo '\|[[:space:]]*head([[:space:]]+(-n[[:space:]]*[0-9]+|-[0-9]+))?' <<<"$command" | tail -n 1)
[ -n "$last" ] || exit 0
limit=$(grep -Eo '[0-9]+' <<<"$last" | tail -n 1)
[ -n "$limit" ] || limit=10

output=$(jq -r '.tool_response
  | if type == "object" then ([.stdout, .output] | map(select(type == "string")) | join("\n"))
    else tostring end' <<<"$input")
lines=$(printf '%s' "$output" | grep -c '')
[ "$lines" -ge "$limit" ] || exit 0

jq -nc --arg c "出力が head の上限($limit 行)に届いた。切られた行がありうる。「ほかにない」「これで全部」と結論する前に、件数を数える(| wc -l、grep -c)か、上限を外して確かめる。" \
  '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $c}}'
