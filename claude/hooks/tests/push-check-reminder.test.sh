#!/usr/bin/env bash
# Cases for push-check-reminder.sh: run `bash claude/hooks/tests/push-check-reminder.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

HOOK="$(cd "$(dirname "$0")/.." && pwd)/push-check-reminder.sh"
failures=0
repo=$(mktemp -d)
other=$(mktemp -d)
plain=$(mktemp -d)
export PUSH_CHECK_STATE_DIR
PUSH_CHECK_STATE_DIR=$(mktemp -d)
trap 'rm -rf "$repo" "$other" "$plain" "$PUSH_CHECK_STATE_DIR"' EXIT
for r in "$repo" "$other"; do
  git -C "$r" init -q
  git -C "$r" -c user.name=t -c user.email=t@t commit -q --allow-empty -m one
done

# expect <deny|pass> <name> <command> [session] [cwd]
expect() {
  local want=$1 name=$2 command=$3 session=${4:-s1} cwd=${5:-$plain} out got
  out=$(jq -nc --arg c "$command" --arg s "$session" --arg cwd "$cwd" \
    '{tool_name: "Bash", session_id: $s, cwd: $cwd, tool_input: {command: $c}}' | bash "$HOOK")
  if grep -q '"permissionDecision":"deny"' <<<"$out"; then got=deny; elif [ -z "$out" ]; then got=pass; else got="other:$out"; fi
  if [ "$got" = "$want" ]; then
    printf 'OK %-5s %s\n' "$got" "$name"
  else
    printf 'NG %-5s (want %s) %s\n' "$got" "$want" "$name"
    failures=$((failures + 1))
  fi
}

expect pass not-a-push "cd $repo && git status"
expect pass push-word-only "cd $repo && echo pushed"
expect deny first-push "cd $repo && git push origin HEAD:main"
expect pass same-head-again "cd $repo && git push origin HEAD:main 2>&1 | tail -1"
expect pass same-head-git-C "git -C $repo push"
expect deny other-session "cd $repo && git push" s2
expect deny other-repo "git -C $other push"
expect pass relative-cd "cd $(basename "$other") && git push" s1 "$(dirname "$other")"
git -C "$repo" -c user.name=t -c user.email=t@t commit -q --allow-empty -m two
expect deny new-head "cd $repo && git push"
expect pass new-head-again "cd $repo && git push"
expect deny no-repo "cd $plain && git push"
expect pass no-repo-again "cd $plain && git push"

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "all cases passed"
