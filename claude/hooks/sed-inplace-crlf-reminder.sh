#!/usr/bin/env bash
# sed-inplace-crlf-reminder.sh — PreToolUse hook (Bash):
# Git Bash の `sed -i` は、置換していない行も含めて CRLF のファイルを全行 LF にする
# (docs/bash.md に記載済みだが、読まずに踏んだ)。
# `sed -i` / `sed --in-place` を含むコマンドの直前だけ、その旨を Claude の文脈に差し込む。止めない。
command=$(jq -r '.tool_input.command // ""' | tr -d '\r')
grep -Eq '(^|[[:space:];&|(])sed[[:space:]]+([^|;&]*[[:space:]])?(-[A-Za-z]*i|--in-place)' <<<"$command" || exit 0
cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Git Bash の sed -i は CRLF のファイルを全行 LF にする(置換していない行も)。CRLF のファイルは Edit ツールで直すか、python で bytes のまま置換する。sed -i を使ったら unix2dos -q <file> で戻し、file <file> の「with CRLF line terminators」で確かめる(docs/bash.md)。"}}
JSON
