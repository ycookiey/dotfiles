#!/usr/bin/env bash
# push-check-reminder.sh — PreToolUse hook (Bash):
# `git push` を含むコマンドを、session・repo・HEAD の組ごとに初回だけ止め、CI相当の確認の項目を返す。
# 同じ組の 2 回目は通す(確認を済ませてから同じ push を出し直す)。HEAD が変われば(amend・rebase)また止まる。
# 文脈に差し込むだけの形では、文が push の結果と一緒に届き、確認が push の後になった(2026-10-11)。
# settings の `if` だけでは、複雑なコマンド(heredoc・ループ等)でも呼ばれるため、ここでも中身を見る。
input=$(cat)
command=$(jq -r '.tool_input.command // ""' <<<"$input")
grep -Eq '(^|[^[:alnum:]_-])git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-[:space:]][^[:space:]]*)?)*[[:space:]]+push([[:space:]]|$)' <<<"$command" || exit 0

# push する repo: command の中の最後の `cd <dir>` か `git -C <dir>`、なければ hook の cwd。
cwd=$(jq -r '.cwd // ""' <<<"$input")
dir=$(grep -Eo '(^|[;&|[:space:]])cd[[:space:]]+[^;&|[:space:]]+' <<<"$command" | tail -n 1 | sed -E 's/^.*cd[[:space:]]+//')
[ -n "$dir" ] || dir=$(grep -Eo 'git[[:space:]]+-C[[:space:]]+[^[:space:]]+' <<<"$command" | tail -n 1 | sed -E 's/^git[[:space:]]+-C[[:space:]]+//')
dir=${dir%\"}
dir=${dir#\"}
case $dir in
  '') dir=$cwd ;;
  /* | [A-Za-z]:*) ;;
  *) dir="$cwd/$dir" ;;
esac

# 組の鍵: repo の root と HEAD。読めなければ command の文字そのもの。
session=$(jq -r '.session_id // ""' <<<"$input")
if repo=$(git -C "$dir" rev-parse --show-toplevel HEAD 2>/dev/null); then
  key="$session $repo"
else
  key="$session $cwd $command"
fi
state=${PUSH_CHECK_STATE_DIR:-${TMPDIR:-/tmp}/claude-push-checked}
mark="$state/$(printf '%s' "$key" | sha1sum | cut -c1-16)"
[ -e "$mark" ] && exit 0
mkdir -p "$state" && : >"$mark"

jq -nc --arg r "push 前の確認(この repo のこの commit で初回の push なので一度止めた。済んだら同じコマンドを出し直せば通る): このrepoのCI workflowの run: 行(package.jsonのscript: typecheck / lint / format:check / test / build 等)をそのまま回したか。toolの単体呼び出しで代用しない(例: eslint単体は同じlint scriptの前段oxlintを飛ばす)。Windowsのautocrlfでformat:checkが落ちる場合は ~/.claude/docs/git.md の手順(コミットした中身で確かめる)。push 先の決まり(他の session への知らせ・ユーザーの了承)も満たしたか。(push後の作業は post-push-reminder.sh が push の直後に知らせる)" \
  '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: $r}}'
