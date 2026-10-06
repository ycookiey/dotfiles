#!/usr/bin/env bash
# language-guard.sh — Stop hook: when the turn's final reply is mostly English,
# block the stop and ask Claude to rewrite it in Japanese.
#
# What is judged: the text after the turn's last tool call (from the
# transcript) and `last_assistant_message`. Code blocks, inline code,
# blockquotes, URLs, e-mail addresses and Markdown links are left out before
# counting. A text is English when it has at least LANGUAGE_GUARD_MIN_LATIN
# Latin letters and more than LANGUAGE_GUARD_RATIO times as many Latin letters
# as Japanese characters. A rewrite is never blocked again
# (`stop_hook_active`), so the guard cannot loop.
#
# The short lines between tool calls are not seen: only the turn's final text.
# The approach follows minorun365/claude-code-japanese-guard (Apache-2.0);
# this is an independent bash version. Each process start costs about 0.1s on
# Windows, so the work is kept to a few jq and awk runs.
#
# Usage as a Stop hook: reads the hook input on stdin.
# Check past sessions:  language-guard.sh --check <transcript.jsonl>
#   prints each turn whose final text it would block, with its first line.

set -uo pipefail
# Multi-byte character classes need a UTF-8 locale (see label-guard.sh).
# jq runs with -b: on Windows it would otherwise end lines with CR.
export LC_ALL=C.UTF-8

MIN_LATIN=${LANGUAGE_GUARD_MIN_LATIN:-25}
# Measured on real sessions: English replies had no Japanese at all, while
# Japanese replies full of identifiers, file names and commit IDs reached 10
# Latin letters per Japanese character. 20 leaves room on both sides.
RATIO=${LANGUAGE_GUARD_RATIO:-20}
WAIT_SECONDS=${LANGUAGE_GUARD_WAIT:-3}
# The hook reads only the end of the transcript: the final text of a turn
# comes after its last tool call, near the end.
TAIL_LINES=400

# Each turn's final texts, output as `<label><TAB><text>` records ended by
# NUL. A user entry starts a turn when it is typed text: not only tool
# results, not a meta message. A tool call drops the texts before it.
JQ_TURNS='
def user_turn:
  .type == "user" and ((.isMeta // false) | not) and
  (.message.content | if type == "string" then true
    else (map(.type) | any(. != "tool_result")) end);
reduce inputs as $e ({final: [], turns: []};
  if ($e | user_turn) then .turns += [.final] | .final = []
  elif $e.type == "assistant" then
    reduce (($e.message.content // []) | if type == "array" then .[] else empty end) as $b (.;
      if $b.type == "tool_use" then .final = []
      elif $b.type == "text" then .final += [$b.text]
      else . end)
  else . end)'
# Every turn, numbered from 1 (turn 1 is what came before the first prompt).
JQ_EVERY_TURN="$JQ_TURNS"'
| .turns + [.final] | to_entries[] | (.key + 1) as $turn | .value[]
| "\($turn)\t\(.)\u0000"'
# The last turn only.
JQ_LAST_TURN="$JQ_TURNS"'
| .final[] | "transcript\t\(.)\u0000"'

# For each NUL-ended record `<label>\t<text>`, print `<label>\t<first line>`
# when the text is mostly English. The prose is what is left after removing
# code blocks, blockquotes, inline code, Markdown links, URLs and e-mail
# addresses.
JUDGE_AWK='
BEGIN { RS = "\0" }
{
  tab = index($0, "\t")
  label = substr($0, 1, tab - 1)
  n = split(substr($0, tab + 1), lines, "\n")
  prose = ""; head = ""; in_fence = 0
  for (i = 1; i <= n; i++) {
    line = lines[i]
    if (head == "" && line ~ /[^[:space:]]/) head = substr(line, 1, 80)
    if (line ~ /^[[:space:]]*```/) { in_fence = !in_fence; continue }
    if (in_fence || line ~ /^[[:space:]]*>/) continue
    gsub(/`[^`]*`/, "", line)
    gsub(/\[[^]]*\]\([^)]*\)/, "", line)
    gsub(/https?:\/\/[^[:space:]]+/, "", line)
    gsub(/[[:alnum:]._%+-]+@[[:alnum:].-]+\.[[:alpha:]][[:alpha:]]+/, "", line)
    prose = prose line "\n"
  }
  latin = gsub(/[A-Za-z]/, "&", prose)
  ja = gsub(/[ぁ-んァ-ヶー一-龥々]/, "&", prose)
  if (latin >= min && latin > ja * ratio) printf "%s\t%s\n", label, head
}'

judge() {
  awk -v min="$MIN_LATIN" -v ratio="$RATIO" "$JUDGE_AWK"
}

# Exit 0 when the transcript's last assistant entry ends with text, i.e.
# the final reply has been written.
ended_in_text() {
  tail -n 50 "$1" | jq -bsr '
    [.[] | select(.type == "assistant")] | last
    | (.message.content // []) | if type == "array" then last.type else "" end
  ' 2>/dev/null | grep -qx text
}

if [ "${1:-}" = "--check" ]; then
  transcript=${2:?"usage: language-guard.sh --check <transcript.jsonl>"}
  hits=$(jq -bjn "$JQ_EVERY_TURN" < "$transcript" 2>/dev/null | judge)
  if [ -z "$hits" ]; then
    echo "英語主体の返答はありません"
  else
    printf '%s\n' "$hits" | sed 's/^/turn /; s/\t/: /'
  fi
  exit 0
fi

input=$(cat)
IFS=$'\t' read -r active transcript < <(printf '%s' "$input" |
  jq -br '[(.stop_hook_active // false), (.transcript_path // "")] | @tsv')
[ "$active" = "true" ] && exit 0

hits=$(printf '%s' "$input" |
  jq -bj '(.last_assistant_message // "") | "message\t\(.)\u0000"' | judge)

if [ -n "$transcript" ] && [ -f "$transcript" ]; then
  # Without an English message to go on, wait for the final reply to reach
  # the transcript before reading it.
  if [ -z "$hits" ]; then
    deadline=$((SECONDS + WAIT_SECONDS))
    until ended_in_text "$transcript" || [ "$SECONDS" -ge "$deadline" ]; do
      sleep 0.1
    done
  fi
  final=$(tail -n "$TAIL_LINES" "$transcript" |
    jq -bjn "$JQ_LAST_TURN" 2>/dev/null | judge)
  hits=$(printf '%s\n%s\n' "$hits" "$final" | sed '/^$/d')
fi

[ -z "$hits" ] && exit 0

# The first lines, each once, at most five.
quoted=$(printf '%s\n' "$hits" | cut -f2- | awk '!seen[$0]++' | head -n 5 | sed 's/^/- /')

# Kept for checking false positives afterwards; overwritten on each block.
log_dir="${HOME}/.claude/logs"
mkdir -p "$log_dir" 2>/dev/null
printf '%s' "$input" > "$log_dir/language-guard-last-block.json"

jq -n --arg quoted "$quoted" '{
  decision: "block",
  reason: ("直前の返答が英語主体だった。同じ内容を日本語で書き直し、書き直した返答だけを出す(コード・識別子・引用はそのままでよい)。英語と判定した本文の書き出し:\n" + $quoted)
}'
