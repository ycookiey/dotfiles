r"""PreToolUse hook (Bash), called by backslash-shrink-guard.sh only when the command has `\\`.

Claude Code's Bash tool turns `\\` into a single `\` before bash reads the
command (~/.claude/docs/bash.md). Where bash would have kept the text as
written, that changes what runs:
- a heredoc body (a Python script's "\\n" became a real newline), and
- single quotes and $'...'.
Those are refused, pointing to the Write tool. In double quotes or outside
quotes bash often gives the same result either way, so there it only reminds.
Reads the hook's JSON on stdin; prints the hook's JSON answer or nothing.
"""

import json
import re
import sys

HEREDOC = re.compile(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2")


def split_heredocs(command):
    """The command without heredoc bodies, and the bodies."""
    lines = command.split("\n")
    kept, bodies = [], []
    i = 0
    while i < len(lines):
        line = lines[i]
        kept.append(line)
        i += 1
        for strip_tabs, _, word in HEREDOC.findall(line):
            body = []
            while i < len(lines):
                end = lines[i].lstrip("\t") if strip_tabs else lines[i]
                if end == word:
                    kept.append(lines[i])
                    i += 1
                    break
                body.append(lines[i])
                i += 1
            bodies.append("\n".join(body))
    return "\n".join(kept), bodies


def quoted_places(text):
    """Where each `\\` sits: 'single' (single quotes or $'...') or 'other'."""
    places = []
    state = "out"
    i = 0
    while i < len(text):
        c = text[i]
        pair = text[i : i + 2]
        if state == "single":
            if pair == "\\\\":
                places.append("single")
                i += 2
                continue
            if c == "'":
                state = "out"
        elif state == "ansi":
            if pair == "\\\\":
                places.append("single")
                i += 2
                continue
            if c == "\\":
                i += 2
                continue
            if c == "'":
                state = "out"
        elif state == "double":
            if pair == "\\\\":
                places.append("other")
                i += 2
                continue
            if c == "\\":
                i += 2
                continue
            if c == '"':
                state = "out"
        else:
            if pair == "\\\\":
                places.append("other")
                i += 2
                continue
            if c == "\\":
                i += 2
                continue
            if pair == "$'":
                state = "ansi"
                i += 2
                continue
            if c == "'":
                state = "single"
            elif c == '"':
                state = "double"
        i += 1
    return places


def main():
    # Windows の python の標準入出力は cp932 なので、UTF-8 のバイトで読み書きする。
    data = json.loads(sys.stdin.buffer.read().decode("utf-8"))
    command = (data.get("tool_input") or {}).get("command") or ""
    if "\\\\" not in command:
        return
    rest, bodies = split_heredocs(command)
    in_body = any("\\\\" in body for body in bodies)
    places = quoted_places(rest)
    where = []
    if in_body:
        where.append("heredoc の本文")
    if "single" in places:
        where.append("単一引用('…'・$'…')の中")
    if where:
        reason = (
            "Bash ツールは実行の前に `\\\\` を `\\` 1 本に縮める(~/.claude/docs/bash.md)。"
            + "・".join(where)
            + "の `\\\\` は、書いたとおりには渡らない。中身を Write ツールでファイルに書いてから、"
            "そのファイルを実行する・読ませる(スクリプトはファイルにして python <file>、データはファイルで渡す)。"
        )
        out = {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }
        }
    else:
        out = {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "additionalContext": "このコマンドの `\\\\` は、実行の前に `\\` 1 本に縮んだ(~/.claude/docs/bash.md)。"
                "二重引用の中や引用の外なら bash が同じ結果にすることが多いが、出力が意図どおりか確かめる。",
            }
        }
    sys.stdout.buffer.write((json.dumps(out, ensure_ascii=False) + "\n").encode("utf-8"))


main()
