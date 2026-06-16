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

SESSION_FILE=$(find -L "$CFG_DIR/projects" -name "$CLAUDE_CODE_SESSION_ID.jsonl" -type f 2>/dev/null | head -1)
if [[ -z "$SESSION_FILE" ]]; then
    echo "Session JSONL not found for $CLAUDE_CODE_SESSION_ID under $CFG_DIR/projects" >&2
    exit 1
fi

jq -r --argjson w "$WIDTH" '
    select(.type == "user" and (.isSidechain | not) and (.message.content | type == "string"))
    | (.message.content | gsub("\r"; "") | gsub("\n"; " ") | gsub(" +"; " ")) as $c
    | [$c[0:$w], ($c | length > $w)] | @tsv
' "$SESSION_FILE" | awk -F'\t' '
{
    n++
    suffix = ($2 == "true") ? "..." : ""
    printf "#%d  %s%s\n", n, $1, suffix
}
END {
    printf "\nTotal: %d user messages (session: %s)\n", n, ENVIRON["CLAUDE_CODE_SESSION_ID"]
}
'
