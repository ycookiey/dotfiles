#!/usr/bin/env bash
# Cases for askuser-label-guard.sh: run `bash claude/hooks/tests/askuser-label-guard.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

GUARD="$(cd "$(dirname "$0")/.." && pwd)/askuser-label-guard.sh"
failures=0

# expect <block|pass> <name> <labels(改行区切り)>
expect() {
  local want=$1 name=$2 labels=$3 out got
  out=$(jq -nc --arg l "$labels" \
    '{tool_name: "AskUserQuestion",
      tool_input: {questions: [{question: "q", header: "h", multiSelect: false,
        options: ($l | split("\n") | map({label: ., description: "d"}))}]}}' | bash "$GUARD")
  if [ -n "$out" ]; then got=block; else got=pass; fi
  if [ "$got" = "$want" ]; then
    printf 'OK %-5s %s\n' "$got" "$name"
  else
    printf 'NG %-5s (want %s) %s %s\n' "$got" "$want" "$name" "$out"
    failures=$((failures + 1))
  fi
}

expect pass ids-with-space "A 並列化（推奨）
B 先行起動"
expect pass ids-with-punct "A: 並列化
B+C 両方
C（推奨）"
expect pass numbered-ids "L1 hook
L2 docs"
expect pass id-only "A
B"
expect block no-id "並列化
先行起動"
expect block partly-missing "A 並列化
先行起動"
# 英単語の先頭文字は ID として数えない
expect block word-start "Apply
Both"
expect block lowercase "a 並列化
b 先行起動"

# 入力が壊れていても止めない
out=$(printf 'not json' | bash "$GUARD")
if [ -z "$out" ]; then echo "OK pass  invalid-input"; else echo "NG invalid-input"; failures=$((failures + 1)); fi

[ "$failures" -eq 0 ] || exit 1
