#!/usr/bin/env bash
# Cases for stage-own-edit-reminder.sh: run `bash claude/hooks/tests/stage-own-edit-reminder.test.sh`.
# Builds a throwaway repo, prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

HOOK="$(cd "$(dirname "$0")/.." && pwd)/stage-own-edit-reminder.sh"
failures=0
work=$(mktemp -d)
export TMPDIR="$work/tmp"
mkdir -p "$TMPDIR"
trap 'rm -rf "$work"' EXIT

repo="$work/repo"
git init -q "$repo"
git -C "$repo" config user.email test@example.com
git -C "$repo" config user.name test
mkdir -p "$repo/sub"
printf 'a\n' >"$repo/clean.md"
printf 'a\n' >"$repo/sub/dirty.md"
printf 'a\n' >"$repo/begun.md"
git -C "$repo" add clean.md sub/dirty.md begun.md
git -C "$repo" commit -q -m init
printf 'b\n' >>"$repo/sub/dirty.md"
printf 'b\n' >>"$repo/begun.md"
printf 'a\n' >"$repo/untracked.md"

# simulate `git stage-own-edit begin begun.md`
store="$repo/$(git -C "$repo" rev-parse --git-path stage-own-edit)"
mkdir -p "$store"
cp "$repo/begun.md" "$store/$(printf '%s' begun.md | sha1sum | cut -c1-40)"

# expect <remind|quiet> <name> <file_path> [session]
expect() {
  local want=$1 name=$2 path=$3 session=${4:-s1} out got
  out=$(jq -nc --arg p "$path" --arg s "$session" '{tool_name: "Edit", session_id: $s, tool_input: {file_path: $p}}' | bash "$HOOK")
  if [ -n "$out" ]; then got=remind; else got=quiet; fi
  if [ "$got" = "$want" ]; then
    printf 'OK %-6s %s\n' "$got" "$name"
  else
    printf 'NG %-6s (want %s) %s %s\n' "$got" "$want" "$name" "$out"
    failures=$((failures + 1))
  fi
}

expect remind dirty-tracked "$repo/sub/dirty.md"
expect quiet same-session-again "$repo/sub/dirty.md"
expect remind other-session "$repo/sub/dirty.md" s2
expect quiet clean-tracked "$repo/clean.md"
expect quiet untracked "$repo/untracked.md"
expect quiet new-file "$repo/new.md"
expect quiet already-begun "$repo/begun.md"
expect quiet outside-repo "$work/tmp"

if [ "$failures" -gt 0 ]; then
  echo "$failures case(s) failed"
  exit 1
fi
echo "all cases passed"
