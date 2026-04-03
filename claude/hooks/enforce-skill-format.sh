#!/bin/bash
# Block .claude/commands/ writes and redirect to .claude/skills/*/SKILL.md format.
jq -n '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "block",
    reason: ".claude/commands/ is deprecated in this environment. Use .claude/skills/<name>/SKILL.md instead. Format: YAML frontmatter (name, description) + Markdown body. No further research needed."
  }
}'
