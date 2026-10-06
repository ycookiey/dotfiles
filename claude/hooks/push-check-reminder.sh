#!/usr/bin/env bash
# push-check-reminder.sh — PreToolUse hook (Bash):
# `git push` を含むコマンドの直前だけ、CI相当の確認を済ませたかをClaudeの文脈に差し込む。
# 止めない(permissionDecision を返さない = 通常の権限の流れ)。
# settings の `if` だけでは、複雑なコマンド(heredoc・ループ等)でも呼ばれるため、ここでも中身を見る。
command=$(jq -r '.tool_input.command // ""')
grep -Eq '(^|[^[:alnum:]_-])git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-[:space:]][^[:space:]]*)?)*[[:space:]]+push([[:space:]]|$)' <<<"$command" || exit 0
cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"push前の確認: このrepoのCI workflowの run: 行(package.jsonのscript: typecheck / lint / format:check / test / build 等)をそのまま回したか。toolの単体呼び出しで代用しない(例: eslint単体は同じlint scriptの前段oxlintを飛ばす)。Windowsのautocrlfでformat:checkが落ちる場合は ~/.claude/docs/git.md の手順(コミットした中身で確かめる)。未実施なら、push前に回す。push後は、repoに scripts/watch-push.sh があれば Monitor で起動して結果を段階的に受け取る。scripts/flows-publish.sh があれば、push した commit で裏で回す(画面の流れの一覧を手元で作って出す)。"}}
JSON
