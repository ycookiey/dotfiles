#!/usr/bin/env bash
# Cases for sed-inplace-crlf-reminder.sh: run `bash claude/hooks/tests/sed-inplace-crlf-reminder.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

HOOK="$(cd "$(dirname "$0")/.." && pwd)/sed-inplace-crlf-reminder.sh"
failures=0

# expect <remind|quiet> <name> <command>
expect() {
  local want=$1 name=$2 command=$3 out got
  out=$(jq -nc --arg c "$command" '{tool_name: "Bash", tool_input: {command: $c}}' | bash "$HOOK")
  if [ -n "$out" ]; then got=remind; else got=quiet; fi
  if [ "$got" = "$want" ]; then
    printf 'OK %-6s %s\n' "$got" "$name"
  else
    printf 'NG %-6s (want %s) %s %s\n' "$got" "$want" "$name" "$out"
    failures=$((failures + 1))
  fi
}

expect remind dash-i "sed -i 's/a/b/' file.md"
expect remind dash-i-backup "sed -i.bak 's/a/b/' file.md"
expect remind combined-flags "sed -Ei 's/a+/b/' file.md"
expect remind flag-before "sed -E -i 's/a+/b/' file.md"
expect remind long-option "sed --in-place 's/a/b/' file.md"
expect remind after-cd "cd repo && sed -i 's/a/b/' x.yml"
expect quiet pipe-filter "git diff | sed 's/\r\$//'"
expect quiet print-range "sed -n 1,20p file.md"
expect quiet extended "sed -E 's/(a)/\1/' file.md"
expect quiet no-sed "grep -i sed notes.md"

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "all cases passed"
