#!/usr/bin/env bash
# rm-junction-guard.sh — PreToolUse hook (Bash):
# pnpm の node_modules は junction(Windows のリンク)を含み、Git Bash の rm -r は
# その先(link: 依存の兄弟の checkout など)までたどって消しうる。
# 再帰の rm の対象が node_modules を含むフォルダ(またはそれ自体)なら止め、
# cmd //c "rmdir /s /q ..." を使うよう返す(rmdir は junction のリンクだけを消す)。
# 変数を含むなどで場所が読めない対象は止めない。
input=$(cat)
command=$(jq -r '.tool_input.command // ""' <<<"$input")
cwd=$(jq -r '.cwd // ""' <<<"$input")
# Only rm at the start of a command (not git rm and the like).
start='(^|[;&|(]|then|do|else)[[:space:]]*'
grep -Eq "${start}rm[[:space:]]" <<<"$command" || exit 0

# Each rm call: from "rm" up to the next ; & | or newline.
calls=$(grep -oE "${start}rm[[:space:]][^;&|]*" <<<"$command")
blocked=""
while IFS= read -r call; do
  [ -z "$call" ] && continue
  words=$(sed -E 's/^([;&|(]|then|do|else)?[[:space:]]*rm[[:space:]]+//' <<<"$call")
  is_recursive=false
  targets=()
  for word in $words; do
    word=${word//\"/}
    word=${word//\'/}
    case "$word" in
      --recursive) is_recursive=true ;;
      --*) ;;
      -*) [[ "$word" == *[rR]* ]] && is_recursive=true ;;
      *) targets+=("$word") ;;
    esac
  done
  $is_recursive || continue
  for target in "${targets[@]}"; do
    # Unreadable places (variables, globs, ~ forms) are left alone.
    [[ "$target" == *'$'* || "$target" == *'*'* || "$target" == *'`'* ]] && continue
    case "$target" in
      /*) path=$target ;;
      [A-Za-z]:*) path=$target ;;
      *) path="${cwd:-.}/$target" ;;
    esac
    [ -d "$path" ] || continue
    if [ -n "$(find "$path" -maxdepth 3 -name node_modules -type d -print -quit 2>/dev/null)" ]; then
      blocked="$target"
      break 2
    fi
  done
done <<<"$calls"

[ -z "$blocked" ] && exit 0
reason="rm -r の対象 ${blocked} は node_modules を含む。pnpm の node_modules の junction を rm がたどると、link: 依存の先(兄弟の checkout 等)まで消しうる。cmd //c \"rmdir /s /q C:\\\\path\" で消す(junction はリンクだけ消える)。詳細 ~/.claude/docs/git.md の「部分 stage の単独検証と worktree の後片付け」"
jq -n --arg reason "$reason" '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: $reason}}'
