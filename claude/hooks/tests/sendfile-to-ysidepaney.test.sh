#!/usr/bin/env bash
# Cases for sendfile-to-ysidepaney.sh: run `bash claude/hooks/tests/sendfile-to-ysidepaney.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

HOOK="$(cd "$(dirname "$0")/.." && pwd)/sendfile-to-ysidepaney.sh"
failures=0
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

# A stand-in for ysidepaney: records each call, fails when FAKE_FAIL is set.
cat >"$root/fake" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$FAKE_LOG"
[ -z "${FAKE_FAIL:-}" ]
SH
chmod +x "$root/fake"
export YSIDEPANEY_BIN="$root/fake" FAKE_LOG="$root/log"

# expect <deny|pass> <posts> <name> <files JSON> [caption] [log must contain]
expect() {
  local want=$1 posts=$2 name=$3 files=$4 caption=${5:-} needle=${6:-} out got count
  : >"$FAKE_LOG"
  out=$(jq -nc --argjson f "$files" --arg c "$caption" \
    '{tool_name: "SendUserFile", cwd: "C:/work", session_id: "s1", tool_input: {files: $f, caption: $c, status: "normal"}}' |
    bash "$HOOK")
  if grep -q '"permissionDecision":"deny"' <<<"$out"; then got=deny; else got=pass; fi
  count=$(grep -c . "$FAKE_LOG")
  if [ "$got" = "$want" ] && [ "$count" = "$posts" ] && { [ -z "$needle" ] || grep -qF -- "$needle" "$FAKE_LOG"; }; then
    printf 'OK %-4s %s\n' "$got" "$name"
  else
    printf 'NG %-4s (want %s, posts %s want %s) %s %s\n' "$got" "$want" "$count" "$posts" "$name" "$(cat "$FAKE_LOG")"
    failures=$((failures + 1))
  fi
}

expect deny 1 one-png '["C:\\shots\\a.png"]' "比較の画像" 'post image C:\shots\a.png --title 比較の画像 --session s1'
expect deny 2 two-images '["/tmp/a.PNG", "/tmp/b.jpg"]' "" '--title a.PNG --session'
expect deny 2 two-with-caption '["/tmp/a.png", "/tmp/b.png"]' "PC と スマホ" '--title PC と スマホ (b.png)'
expect deny 1 relative '["shots/c.webp"]' "" 'post image C:/work/shots/c.webp'
expect deny 1 mixed '["a.png", "report.html"]' "見て"
expect pass 0 html-only '["report.html"]' "報告"
expect pass 0 no-files '[]'
FAKE_FAIL=1 expect pass 1 post-fails '["a.png"]'
YSIDEPANEY_BIN="$root/missing" expect pass 0 no-ysidepaney '["a.png"]'

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "all cases passed"
