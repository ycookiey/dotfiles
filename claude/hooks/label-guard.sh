#!/bin/bash
# Stop hook. When the assistant's last message uses Greek letters or Roman
# numeral list markers as identification labels, block the stop event and ask
# Claude to rewrite using A, B, C, ... labels.

set -uo pipefail

input=$(cat)
text=$(printf '%s' "$input" | jq -r '.last_assistant_message // empty')

if [ -z "$text" ]; then
  exit 0
fi

# Strip fenced code blocks, blockquotes, and inline backtick spans so mentions
# of forbidden markers inside code or quotes do not trigger the guard.
sanitized=$(printf '%s\n' "$text" | awk '
  BEGIN { in_fence = 0 }
  /^```/ { in_fence = !in_fence; next }
  in_fence { next }
  /^[[:space:]]*>/ { next }
  {
    gsub(/`[^`]*`/, "")
    print
  }
')

violations=()

# Greek letter used as a label. Line-start Greek requires whitespace or a
# label delimiter after it so scientific terms like "α粒子" / "β分布" do not
# trigger. Anywhere in the line, Greek followed by a strong label delimiter
# or a Japanese label suffix also counts.
if printf '%s\n' "$sanitized" | grep -qE '(^[[:space:]]*[-*+]?[[:space:]]*[αβγδεζ][[:space:].\):、：]|[αβγδεζ][.\):]|[αβγδεζ](案|方式|パターン|プラン|型))'; then
  violations+=("Greek letters (α, β, γ, ...)")
fi

# Roman numeral list marker at line start with a period or paren delimiter.
if printf '%s\n' "$sanitized" | grep -qE '^[[:space:]]*[-*+]?[[:space:]]*(i{1,3}|iv|v|vi{1,3})[.\)]'; then
  violations+=("Roman numerals (i., ii., iii., ...)")
fi

if [ ${#violations[@]} -eq 0 ]; then
  exit 0
fi

joined=$(IFS=", "; printf '%s' "${violations[*]}")

jq -n --arg reason "Forbidden label style detected in your previous response: ${joined}. Rewrite that response now, replacing the offending labels with A, B, C, ... . Deliver only the corrected response." '{
  decision: "block",
  reason: $reason
}'
