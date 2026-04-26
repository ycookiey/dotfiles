# Claude Code MCP サーバー管理

dotfiles 管理。bootstrap 対象外。シェル起動ごとの常駐同期で反映。

## 仕組み

- 定義: `dotfiles/claude/mcp-servers.json`（プレースホルダ付きテンプレート）
- 同期: `dotcli sync` がプレースホルダ展開 → `~/.claude.json` の `mcpServers` に差分注入
- 発火: PowerShell は `dotfiles/pwsh/profile.ps1` のプロンプトフック、Nushell は `dotfiles/nushell/config.nu` の起動時 job spawn
- スコープ: user（`claude mcp add -s user` と同位置）

プレースホルダ一覧は `dotfiles/cli/src/commands/sync.rs` 参照。

## 追加/削除

1. `mcp-servers.json` を編集
2. 次回シェル起動で自動反映 → Claude Code 再起動で認識

`claude mcp add` は使わない（sync の差分判定と競合）。`~/.claude.json` 直編集も次回 sync で上書き。
