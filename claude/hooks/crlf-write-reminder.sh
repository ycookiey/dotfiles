#!/usr/bin/env bash
# crlf-write-reminder.sh — PreToolUse hook (Bash):
# Windows の Python は text mode の書き込み(`Path.write_text` / `open(p, "w")`)で
# `\n` を `\r\n` に変える。LF のファイルを置き換えると全行が CRLF になり、
# formatter の確認や git の差分が全行で崩れる(docs/bash.md に記載済みだが、読まずに踏んだ)。
# そうした書き込みを含むコマンドの直前だけ、その旨を Claude の文脈に差し込む。
# `newline=` を指定しているコマンドは対象外。止めない(permissionDecision を返さない)。
command=$(jq -r '.tool_input.command // ""')
grep -Eq 'write_text\(|open\([^)]*["'"'"'](w|a|x)[t+]?["'"'"']' <<<"$command" || exit 0
grep -q 'newline=' <<<"$command" && exit 0
cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Windows の Python は text mode の書き込み(Path.write_text / open(p, \"w\"))で \\n を \\r\\n に変える。LF のファイルを書き換えると全行が CRLF になる。read_bytes().decode() → write_bytes(s.encode())、または open(p, \"w\", newline=\"\") を使う。書いた後は file <path> か git ls-files --eol <path> で確かめる(docs/bash.md)。"}}
JSON
