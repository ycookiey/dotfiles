#!/usr/bin/env bash
# worktree-remove-reminder.sh — PreToolUse hook (Bash):
# `git worktree remove` を含むコマンドの直前だけ、Windows + pnpm での後片付けの罠を Claude の文脈に差し込む。
# 止めない(permissionDecision を返さない = 通常の権限の流れ)。
# settings の `if` だけでは、複雑なコマンド(heredoc・ループ等)でも呼ばれるため、ここでも中身を見る。
command=$(jq -r '.tool_input.command // ""')
grep -Eq '(^|[^[:alnum:]_-])git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-[:space:]][^[:space:]]*)?)*[[:space:]]+worktree[[:space:]]+remove([[:space:]]|$)' <<<"$command" || exit 0
cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"worktree の後片付け: pnpm の node_modules(junction)があると git worktree remove は \"Directory not empty\" で失敗するが、登録は外れる。残ったフォルダは cmd //c \"rmdir /s /q C:\\path\" で消す(junction はリンクだけ消し、先をたどらない)。rm -rf は junction の扱いが環境依存で、link: 依存の先(兄弟の checkout 等)を消しうるので使わない。詳細 ~/.claude/docs/git.md の「部分 stage の単独検証と worktree の後片付け」"}}
JSON
