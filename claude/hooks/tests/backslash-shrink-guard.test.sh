#!/usr/bin/env bash
# Cases for backslash-shrink-guard.sh: run `bash claude/hooks/tests/backslash-shrink-guard.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.
# The commands are written here as they reach the hook: `\\` is two backslashes.

set -uo pipefail
export LC_ALL=C.UTF-8

HOOK="$(cd "$(dirname "$0")/.." && pwd)/backslash-shrink-guard.sh"
failures=0

# expect <deny|remind|pass> <name>   (the command on stdin)
expect() {
  local want=$1 name=$2 command out got
  command=$(cat)
  out=$(jq -nc --arg c "$command" '{tool_name: "Bash", tool_input: {command: $c}}' | bash "$HOOK")
  if grep -q '"permissionDecision": *"deny"' <<<"$out"; then got=deny
  elif [ -n "$out" ]; then got=remind
  else got=pass; fi
  if [ "$got" = "$want" ] && { [ -z "$out" ] || jq -e . >/dev/null 2>&1 <<<"$out"; }; then
    printf 'OK %-6s %s\n' "$got" "$name"
  else
    printf 'NG %-6s (want %s) %s %s\n' "$got" "$want" "$name" "$out"
    failures=$((failures + 1))
  fi
}

expect deny heredoc-python <<'CASE'
python - <<'EOF'
print("CR は \\r\\n")
EOF
CASE
expect deny heredoc-unquoted-word <<'CASE'
cat <<EOF > a.txt
C:\\Users
EOF
CASE
expect deny heredoc-dash <<'CASE'
cat <<-END
	a\\b
	END
CASE
expect deny single-quote <<'CASE'
printf '%s\n' 'a\\b'
CASE
expect deny ansi-c <<'CASE'
echo $'a\\b'
CASE
expect remind double-quote <<'CASE'
grep -n "a\\.b" file.txt
CASE
expect remind bare <<'CASE'
echo a\\b
CASE
expect remind after-heredoc <<'CASE'
cat <<'EOF'
plain
EOF
grep "x\\.y" f
CASE
expect pass no-backslash <<'CASE'
ls -la /c/Main
CASE
expect pass single-backslash <<'CASE'
printf '%s\n' 'a\b' "C:\Main"
CASE
expect pass heredoc-single-backslash <<'CASE'
python - <<'EOF'
print("a\nb")
EOF
CASE
expect pass single-quote-in-double <<'CASE'
echo "it's \"ok\""
CASE

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "all cases passed"
