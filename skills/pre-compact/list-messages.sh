#!/usr/bin/env bash
# List user input messages of the current Claude Code session with numbers.
# Used by the `pre-compact` skill to identify compact target messages.
#
# Env: CLAUDE_CODE_SESSION_ID, CLAUDE_CONFIG_DIR (set by Claude Code).
# Usage: list-messages.sh [--width N]

set -euo pipefail

WIDTH=80
while [[ $# -gt 0 ]]; do
    case "$1" in
        --width) WIDTH="$2"; shift 2 ;;
        -h|--help)
            cat <<EOF
Usage: $(basename "$0") [--width N]
  --width N   Preview character count (default: 80)
EOF
            exit 0 ;;
        *) echo "Unknown arg: $1" >&2; exit 2 ;;
    esac
done

: "${CLAUDE_CODE_SESSION_ID:?CLAUDE_CODE_SESSION_ID is not set}"
: "${CLAUDE_CONFIG_DIR:?CLAUDE_CONFIG_DIR is not set}"

CFG_DIR=$(cygpath -u "$CLAUDE_CONFIG_DIR")

# A session file always sits at projects/<project>/<id>.jsonl, so a depth-2 glob
# finds it without walking the whole tree (subagent logs live deeper).
SESSION_FILE=
shopt -s nullglob
for f in "$CFG_DIR"/projects/*/"$CLAUDE_CODE_SESSION_ID".jsonl; do
    if [[ -f "$f" ]]; then SESSION_FILE=$f; break; fi
done
shopt -u nullglob
if [[ -z "$SESSION_FILE" ]]; then
    echo "Session JSONL not found for $CLAUDE_CODE_SESSION_ID under $CFG_DIR/projects" >&2
    exit 1
fi

# grep drops non-user lines before jq parses them (session files are compact
# JSON, so every user record contains the literal '"type":"user"'). Exit 1 means
# no match and must not trip pipefail; real errors (exit 2) still do. -a keeps
# grep from switching to "Binary file matches" on a stray invalid byte.
# jq prints one finished preview per line (newlines are already folded) and awk
# only numbers the lines, so backslashes stay as written. Tabs and NULs are
# still shown as \t and \0 so they do not reach the terminal raw.
# Meta records (isMeta) are kept on purpose, so the numbers match the candidates
# listed by the UI's "Summarize up to here".
{ grep -aF '"type":"user"' "$SESSION_FILE" || [[ $? -eq 1 ]]; } | jq -r --argjson w "$WIDTH" '
    select(.type == "user" and (.isSidechain | not) and (.message.content | type == "string"))
    | (.message.content | gsub("\r"; "") | gsub("\n"; " ") | gsub(" +"; " ")) as $c
    | ($c[0:$w] | gsub("\t"; "\\t") | gsub("\u0000"; "\\0"))
      + (if ($c | length) > $w then "..." else "" end)
' | awk '
{
    n++
    printf "#%d  %s\n", n, $0
}
END {
    printf "\nTotal: %d user messages (session: %s)\n", n, ENVIRON["CLAUDE_CODE_SESSION_ID"]
}
'
