#!/usr/bin/env bash
# Cases for post-push-reminder.sh: run `bash claude/hooks/tests/post-push-reminder.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

HOOK="$(cd "$(dirname "$0")/.." && pwd)/post-push-reminder.sh"
failures=0
repo=$(mktemp -d)
plain=$(mktemp -d)
mkdir -p "$repo/scripts"
touch "$repo/scripts/watch-push.sh" "$repo/scripts/flows-publish.sh"
trap 'rm -rf "$repo" "$plain"' EXIT

# expect <remind|pass> <name> <command> <response-json> [want-text]
expect() {
  local want=$1 name=$2 command=$3 response=$4 text=${5:-} out got
  out=$(jq -nc --arg c "$command" --argjson r "$response" --arg cwd "$plain" \
    '{tool_name: "Bash", cwd: $cwd, tool_input: {command: $c}, tool_response: $r}' | bash "$HOOK")
  if [ -n "$out" ]; then got=remind; else got=pass; fi
  if [ "$got" = "$want" ] && { [ -z "$text" ] || grep -qF -- "$text" <<<"$out"; }; then
    printf 'OK %-6s %s\n' "$got" "$name"
  else
    printf 'NG %-6s (want %s %s) %s %s\n' "$got" "$want" "$text" "$name" "$out"
    failures=$((failures + 1))
  fi
}

pushed='{"stdout": "ff-ok\n   eb44212..6a318de  HEAD -> main\n", "stderr": ""}'
expect remind cd-then-push "cd $repo && git push origin HEAD:main 2>&1 | tail -1" "$pushed" "scripts/flows-publish.sh 6a318de"
expect remind names-both "cd $repo && git push origin HEAD:main" "$pushed" "scripts/watch-push.sh 6a318de"
expect remind git-C "git -C $repo push origin HEAD:main" "$pushed" "6a318de"
expect remind stderr-only "cd $repo && git push" '{"stdout": "", "stderr": "To github.com:o/r.git\n   1a5eeb1..cf4686c  HEAD -> main\n"}' "cf4686c"
expect remind forced "cd $repo && scripts/force-push-safely.sh x && git push --force-with-lease" '{"stdout": " + 1a5eeb1...cf4686c main -> main (forced update)\n"}' "cf4686c"
expect pass no-scripts "cd $plain && git push origin HEAD:main" "$pushed"
expect pass up-to-date "cd $repo && git push" '{"stdout": "Everything up-to-date\n"}'
expect pass rejected "cd $repo && git push" '{"stderr": " ! [rejected]        main -> main (fetch first)\n"}'
expect pass not-a-push "cd $repo && git status" "$pushed"
expect pass push-word-only "cd $repo && echo pushed" "$pushed"

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "all cases passed"
