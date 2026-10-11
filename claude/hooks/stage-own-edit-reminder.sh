#!/usr/bin/env bash
# stage-own-edit-reminder.sh — PreToolUse hook (Edit|Write):
# 他の session・tool の未コミットの変更があるファイルを編集すると、後で自分の編集だけを
# commit するには、編集前に `git stage-own-edit begin <file>` で写しを取っておく必要がある
# (docs/git.md に記載済みだが、読まずに編集し、後から行の切り分けで苦労した)。
# 追跡済みで HEAD との差分があり、まだ begin していないファイルを編集する直前だけ、
# その旨を Claude の文脈に差し込む。同じ session・同じファイルでは 1 回だけ。止めない。
input=$(cat)
path=$(jq -r '.tool_input.file_path // ""' <<<"$input" | tr -d '\r')
session=$(jq -r '.session_id // "none"' <<<"$input" | tr -d '\r')
[ -n "$path" ] || exit 0
command -v cygpath >/dev/null && path=$(cygpath -u "$path")
[ -f "$path" ] || exit 0
dir=$(dirname "$path")
top=$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null) || exit 0
rel=$(git -C "$dir" ls-files --full-name --error-unmatch -- "$(basename "$path")" 2>/dev/null | tr -d '\r') || exit 0
[ -n "$rel" ] || exit 0
git -C "$top" diff --quiet HEAD -- "$rel" 2>/dev/null && exit 0
hash=$(printf '%s' "$rel" | sha1sum | cut -c1-40)
store=$(git -C "$top" rev-parse --git-path stage-own-edit | tr -d '\r')
case "$store" in /* | [A-Za-z]:*) ;; *) store="$top/$store" ;; esac
[ -e "$store/$hash" ] && exit 0
mark="${TMPDIR:-/tmp}/claude-stage-own-edit-reminder/$session/$hash"
[ -e "$mark" ] && exit 0
mkdir -p "$(dirname "$mark")" && : >"$mark"
jq -nc --arg rel "$rel" '{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: ("\($rel) には HEAD からの未コミットの変更がある(他 session の作業かもしれない)。今回の編集を後で単独で commit するなら、編集前に `git stage-own-edit begin \($rel)` を repo の top で実行し、commit 時に `git stage-own-edit stage \($rel)` で自分の行だけを stage する(docs/git.md)。")}}'
