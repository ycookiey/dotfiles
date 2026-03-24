#!/usr/bin/env python3
"""Claude Code conversation history tool.

Subcommands:
  list   - List sessions with metadata
  search - Full-text search across sessions
  read   - Read a specific session's messages
"""

import argparse
import json
import os
import re
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

CLAUDE_DIR = Path.home() / ".claude"
PROJECTS_DIR = CLAUDE_DIR / "projects"


def _parse_timestamp(ts):
    """Parse ISO 8601 timestamp string to datetime."""
    if isinstance(ts, (int, float)):
        return datetime.fromtimestamp(ts / 1000, tz=timezone.utc)
    # "2026-03-24T21:12:47.507Z"
    ts = ts.replace("Z", "+00:00")
    return datetime.fromisoformat(ts)


def _project_dirs(project_filter=None):
    """Yield (project_name, project_path) tuples."""
    if not PROJECTS_DIR.exists():
        return
    for d in PROJECTS_DIR.iterdir():
        if not d.is_dir():
            continue
        if d.name == "__pycache__":
            continue
        if project_filter and project_filter not in d.name:
            continue
        yield d.name, d


def _session_files(project_path):
    """Yield .jsonl session files in a project directory."""
    for f in project_path.glob("*.jsonl"):
        yield f


def _load_first_user_message(session_path):
    """Load the first user message from a session file."""
    with open(session_path, encoding="utf-8") as f:
        for line in f:
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if obj.get("type") == "user":
                content = obj.get("message", {}).get("content", "")
                if isinstance(content, str):
                    text = content
                elif isinstance(content, list):
                    text = next(
                        (
                            c.get("text", "")
                            for c in content
                            if isinstance(c, dict) and c.get("type") == "text"
                        ),
                        "",
                    )
                else:
                    text = ""
                return {
                    "timestamp": obj.get("timestamp"),
                    "cwd": obj.get("cwd", ""),
                    "git_branch": obj.get("gitBranch", ""),
                    "version": obj.get("version", ""),
                    "text": text[:200],
                }
    return None


def cmd_list(args):
    """List sessions with metadata."""
    cutoff = None
    if args.days:
        cutoff = datetime.now(timezone.utc) - timedelta(days=args.days)

    sessions = []
    for proj_name, proj_path in _project_dirs(args.project):
        for sf in _session_files(proj_path):
            mtime = datetime.fromtimestamp(sf.stat().st_mtime, tz=timezone.utc)
            if cutoff and mtime < cutoff:
                continue
            info = _load_first_user_message(sf)
            if not info:
                continue
            sessions.append(
                {
                    "session_id": sf.stem,
                    "project": proj_name,
                    "mtime": mtime.isoformat(),
                    "first_prompt": info["text"],
                    "cwd": info["cwd"],
                    "git_branch": info["git_branch"],
                }
            )

    sessions.sort(key=lambda s: s["mtime"], reverse=True)
    print(json.dumps(sessions, ensure_ascii=False, indent=2))


def cmd_search(args):
    """Search across all session messages."""
    cutoff = None
    if args.days:
        cutoff = datetime.now(timezone.utc) - timedelta(days=args.days)

    if args.regex:
        try:
            pattern = re.compile(args.query, re.IGNORECASE)
        except re.error as e:
            print(json.dumps({"error": f"Invalid regex: {e}"}, ensure_ascii=False))
            sys.exit(1)
    else:
        pattern = re.compile(re.escape(args.query), re.IGNORECASE)
    results = []

    for proj_name, proj_path in _project_dirs(args.project):
        for sf in _session_files(proj_path):
            mtime = datetime.fromtimestamp(sf.stat().st_mtime, tz=timezone.utc)
            if cutoff and mtime < cutoff:
                continue

            with open(sf, encoding="utf-8") as f:
                for line in f:
                    try:
                        obj = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    msg_type = obj.get("type")
                    if msg_type not in ("user", "assistant"):
                        continue

                    texts = _extract_texts(obj)
                    for text in texts:
                        if pattern.search(text):
                            ts = obj.get("timestamp", "")
                            # Context: surrounding text around match
                            match = pattern.search(text)
                            start = max(0, match.start() - 80)
                            end = min(len(text), match.end() + 80)
                            snippet = text[start:end]
                            if start > 0:
                                snippet = "..." + snippet
                            if end < len(text):
                                snippet = snippet + "..."

                            results.append(
                                {
                                    "session_id": sf.stem,
                                    "project": proj_name,
                                    "role": msg_type,
                                    "timestamp": ts,
                                    "snippet": snippet,
                                }
                            )
                            if args.limit and len(results) >= args.limit:
                                print(json.dumps(results, ensure_ascii=False, indent=2))
                                return
                            break  # One match per message is enough

    results.sort(key=lambda r: r.get("timestamp", ""), reverse=True)
    print(json.dumps(results, ensure_ascii=False, indent=2))


def _extract_texts(obj):
    """Extract readable text from a message object."""
    texts = []
    content = obj.get("message", {}).get("content", "")
    if isinstance(content, str):
        texts.append(content)
    elif isinstance(content, list):
        for c in content:
            if isinstance(c, dict):
                if c.get("type") == "text":
                    texts.append(c.get("text", ""))
                elif c.get("type") == "tool_result":
                    # tool_result content can be string or list
                    tc = c.get("content", "")
                    if isinstance(tc, str):
                        texts.append(tc)
                    elif isinstance(tc, list):
                        for item in tc:
                            if isinstance(item, dict) and item.get("type") == "text":
                                texts.append(item.get("text", ""))
            elif isinstance(c, str):
                texts.append(c)
    return texts


def cmd_read(args):
    """Read a specific session's messages."""
    # Find the session file
    session_file = None
    for _proj_name, proj_path in _project_dirs():
        candidate = proj_path / f"{args.session_id}.jsonl"
        if candidate.exists():
            session_file = candidate
            break

    if not session_file:
        print(json.dumps({"error": f"Session {args.session_id} not found"}, ensure_ascii=False))
        sys.exit(1)

    messages = []
    with open(session_file, encoding="utf-8") as f:
        for line in f:
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue

            msg_type = obj.get("type")

            # Filter by role
            if args.role != "all" and msg_type != args.role:
                # When include_tools and role is "all", tool results are included naturally
                continue

            if msg_type == "user":
                content = obj.get("message", {}).get("content", "")
                entry = _build_user_entry(obj, content, args.include_tools)
                if entry:
                    messages.append(entry)

            elif msg_type == "assistant":
                entry = _build_assistant_entry(obj, args.include_tools)
                if entry:
                    messages.append(entry)

    if args.tail:
        messages = messages[-args.tail :]

    print(json.dumps(messages, ensure_ascii=False, indent=2))


def _build_user_entry(obj, content, include_tools):
    """Build a user message entry."""
    ts = obj.get("timestamp", "")
    if isinstance(content, str):
        return {"role": "user", "timestamp": ts, "content": content}

    if isinstance(content, list):
        texts = []
        tools = []
        for c in content:
            if not isinstance(c, dict):
                continue
            if c.get("type") == "text":
                texts.append(c.get("text", ""))
            elif c.get("type") == "tool_result" and include_tools:
                tool_content = c.get("content", "")
                if isinstance(tool_content, list):
                    tool_content = "\n".join(
                        item.get("text", "") for item in tool_content if isinstance(item, dict)
                    )
                tools.append(
                    {
                        "tool_use_id": c.get("tool_use_id", ""),
                        "content": tool_content[:500] if isinstance(tool_content, str) else str(tool_content)[:500],
                        "is_error": c.get("is_error", False),
                    }
                )

        entry = {"role": "user", "timestamp": ts}
        if texts:
            entry["content"] = "\n".join(texts)
        if tools:
            entry["tool_results"] = tools
        # Skip entries that are only tool results when include_tools is off
        if not texts and not tools:
            return None
        return entry

    return None


def _build_assistant_entry(obj, include_tools):
    """Build an assistant message entry."""
    ts = obj.get("timestamp", "")
    content = obj.get("message", {}).get("content", [])

    texts = []
    tools = []
    for c in content:
        if not isinstance(c, dict):
            continue
        if c.get("type") == "text":
            texts.append(c.get("text", ""))
        elif c.get("type") == "tool_use" and include_tools:
            tools.append(
                {
                    "tool": c.get("name", ""),
                    "input_preview": json.dumps(c.get("input", {}), ensure_ascii=False)[:300],
                }
            )
        # Skip thinking blocks

    entry = {"role": "assistant", "timestamp": ts}
    if texts:
        entry["content"] = "\n".join(texts)
    if tools:
        entry["tool_calls"] = tools
    # Skip if no visible content
    if not texts and not tools:
        return None
    return entry


def main():
    parser = argparse.ArgumentParser(description="Claude Code conversation history tool")
    sub = parser.add_subparsers(dest="command", required=True)

    # list
    p_list = sub.add_parser("list", help="List sessions")
    p_list.add_argument("--project", help="Filter by project name (substring match)")
    p_list.add_argument("--days", type=int, default=30, help="Look back N days (default: 30)")

    # search
    p_search = sub.add_parser("search", help="Search messages")
    p_search.add_argument("query", help="Search keyword")
    p_search.add_argument("--project", help="Filter by project name (substring match)")
    p_search.add_argument("--days", type=int, help="Look back N days")
    p_search.add_argument("--limit", type=int, default=20, help="Max results (default: 20)")
    p_search.add_argument("--regex", action="store_true", help="Treat query as regex pattern")

    # read
    p_read = sub.add_parser("read", help="Read a session")
    p_read.add_argument("session_id", help="Session UUID")
    p_read.add_argument("--role", choices=["user", "assistant", "all"], default="all")
    p_read.add_argument("--tail", type=int, help="Show only last N messages")
    p_read.add_argument("--include-tools", action="store_true", help="Include tool calls/results (only with --role all)")

    args = parser.parse_args()

    if args.command == "list":
        cmd_list(args)
    elif args.command == "search":
        cmd_search(args)
    elif args.command == "read":
        # Enforce: include_tools only with role=all
        if hasattr(args, "include_tools") and args.include_tools and args.role != "all":
            print(json.dumps({"error": "--include-tools requires --role all"}, ensure_ascii=False))
            sys.exit(1)
        cmd_read(args)


if __name__ == "__main__":
    main()
