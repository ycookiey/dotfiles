# Cursor Agent CLI

## よくある間違い

- `cursor` コマンドは **GUI 起動**。CLI 操作は `agent` を使う
- `agent` の実体は `agent.cmd`。Git Bash では `command_not_found_handle`（`.bashrc`）経由で `.cmd` を自動解決
- ヘルプは `agent --help`。`cursor --help` / `cursor agent` は GUI が開くだけ

## 設定体系

- ルール: `.cursor/rules/` 配下（Claude の `CLAUDE.md` に相当）
- worktree: `~/.cursor/worktrees/<repo>/<name>`
- MCP: `agent mcp` サブコマンドで管理

## Claude Code との対応

| Claude Code | Cursor Agent |
|---|---|
| `--dangerously-skip-permissions` | `--force` / `--yolo` |
| 対話モード（デフォルト） | 対話モード（デフォルト） |
| `--print`（非対話） | `--print`（非対話） |
| `CLAUDE.md` | `.cursor/rules/` |
| `--resume` | `--resume` / `--continue` |
