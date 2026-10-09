#!/usr/bin/env bash
# Cases for crlf-write-reminder.sh: run `bash claude/hooks/tests/crlf-write-reminder.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

HOOK="$(cd "$(dirname "$0")/.." && pwd)/crlf-write-reminder.sh"
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

expect remind write-text "python - <<'EOF'
Path('a.ts').write_text(s, encoding='utf-8')
EOF"
expect remind open-w-double "python -c 'open(\"a.txt\", \"w\").write(s)'"
expect remind open-w-single "python - <<'EOF'
with open('a.txt', 'w', encoding='utf-8') as f: f.write(s)
EOF"
expect remind open-append "python - <<'EOF'
open('log.txt', 'a').write(line)
EOF"
expect quiet write-bytes "python - <<'EOF'
Path('a.ts').write_bytes(s.encode('utf-8'))
EOF"
expect quiet open-newline "python - <<'EOF'
open('a.txt', 'w', newline='').write(s)
EOF"
expect quiet open-read "python - <<'EOF'
s = open('a.txt').read()
EOF"
expect quiet unrelated "git status --short"

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "all cases passed"
