#!/usr/bin/env bash
# Cases for check-claude-config: run `bash bin/tests/check-claude-config.test.sh`.
# Each case writes one instruction file, runs the checker on it, and
# compares the exit code and whether the expected text was reported.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail

TOOL="$(cd "$(dirname "$0")/.." && pwd)/check-claude-config"
failures=0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# run_case <name> <want exit> <want text or ""> <file content>
run_case() {
  local name=$1 want_exit=$2 want_text=$3 content=$4
  local f="$work/$name/SKILL.md"
  mkdir -p "$work/$name"
  printf '%s\n' "$content" > "$f"
  local out code
  out=$(bash "$TOOL" "$f")
  code=$?
  if [ "$code" -eq "$want_exit" ] && { [ -z "$want_text" ] && [ -z "$out" ] || [ -n "$want_text" ] && [[ "$out" == *"$want_text"* ]]; }; then
    echo "OK $name"
  else
    echo "NG $name: exit=$code out=[$out]"
    failures=$((failures + 1))
  fi
}

run_case task-in-allowed-tools 1 'lists Task' $'---\nname: x\nallowed-tools: Bash(gh issue:*), Task, Read\n---\nbody'
run_case teamcreate-in-tools 1 'lists TeamCreate' $'---\nname: x\ntools: Read, TeamCreate, Agent\n---\nbody'
run_case replace-in-allowed-tools 1 'lists Replace' $'---\nname: x\nallowed-tools: Read, Replace, Write\n---\nbody'
run_case current-tools-only 0 '' $'---\nname: x\nallowed-tools: Read, Agent, SendMessage, TaskCreate, Bash(git status:*)\n---\nbody'
run_case task-prefix-names-are-fine 0 '' $'---\nname: x\ntools: TaskList, TaskUpdate, TaskStop\n---\nbody'
run_case task-in-body-is-not-frontmatter 0 '' $'---\nname: x\n---\nallowed-tools: Task (body text, not frontmatter)'
run_case claudeignore-recommended 1 'mentions .claudeignore' $'---\nname: x\n---\n- .claudeignore未設定 → 作成推奨'
run_case claudeignore-explained 0 '' $'---\nname: x\n---\n- 除外は permissions.deny（Claude Code は .claudeignore を読まない）'
run_case claudeignore-english-explained 0 '' $'---\nname: x\n---\nA .claudeignore file has no effect.'
run_case crlf-frontmatter 1 'lists Task' $'---\r\nname: x\r\nallowed-tools: Task\r\n---\r\nbody'

# directory argument: finds SKILL.md under it
mkdir -p "$work/dir/skills/a"
printf '%s\n' '---' 'tools: Task' '---' > "$work/dir/skills/a/SKILL.md"
dir_out=$(bash "$TOOL" "$work/dir")
if [[ "$dir_out" == *'skills/a/SKILL.md:2: tools lists Task'* ]]; then
  echo "OK directory-argument"
else
  echo "NG directory-argument"
  failures=$((failures + 1))
fi

[ "$failures" -eq 0 ]
