#!/usr/bin/env bash
# Cases for head-truncation-reminder.sh: run `bash claude/hooks/tests/head-truncation-reminder.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

HOOK="$(cd "$(dirname "$0")/.." && pwd)/head-truncation-reminder.sh"
failures=0

lines() { seq 1 "$1" | sed 's/^/packages\/a\/src\/x.ts:/'; }

# expect <remind|pass> <name> <command> <stdout>
expect() {
  local want=$1 name=$2 command=$3 stdout=$4 out got
  out=$(jq -nc --arg c "$command" --arg o "$stdout" \
    '{tool_name: "Bash", tool_input: {command: $c}, tool_response: {stdout: $o, stderr: ""}}' | bash "$HOOK")
  if [ -n "$out" ]; then got=remind; else got=pass; fi
  if [ "$got" = "$want" ]; then
    printf 'OK %-6s %s\n' "$got" "$name"
  else
    printf 'NG %-6s (want %s) %s %s\n' "$got" "$want" "$name" "$out"
    failures=$((failures + 1))
  fi
}

# The 2026-10-07 miss: grep cut at the default 10 lines, a use on the 11th.
expect remind default-ten-full 'grep -rn "Team" packages | head' "$(lines 10)"
expect pass default-ten-short 'grep -rn "Team" packages | head' "$(lines 4)"
expect remind dash-n 'grep -rn x . | head -n 20' "$(lines 20)"
expect remind dash-number 'grep -rn x . | cut -c1-150 | head -30' "$(lines 30)"
expect pass under-limit 'grep -rn x . | head -30' "$(lines 12)"
expect remind spaced-n 'git log --oneline | head -n5' "$(lines 5)"
# A command of several statements: the output cannot be tied to one head.
expect pass several-statements 'ls | head -2; grep -rn x . | head -40' "$(lines 40)"
expect pass and-chain 'python -c "print(1)"; ls .claude/hooks | head' "$(lines 30)"
# A leading cd is allowed: the rest is still one pipeline.
expect remind cd-then-pipeline 'cd /c/repo && grep -rn x . | head -20' "$(lines 20)"
expect remind cd-twice 'cd /c/a && cd b && git grep -n x | head' "$(lines 10)"
expect pass no-head 'grep -rn x .' "$(lines 50)"
expect pass head-on-file 'head -20 README.md' "$(lines 20)"
expect pass tail-not-head 'git log | tail -5' "$(lines 5)"

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "all cases passed"
