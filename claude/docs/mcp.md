# Claude Code MCP サーバー管理

dotfiles 管理。bootstrap 対象外。PS 起動ごとの常駐同期で反映。

## 仕組み

- 定義: `dotfiles/claude/mcp-servers.json`（プレースホルダ付きテンプレート）
- 同期: `dotfiles/pwsh/sync.ps1` がプレースホルダ展開 → `~/.claude.json` の `mcpServers` に差分注入
- 発火: `dotfiles/pwsh/profile.ps1` のプロンプトフック
- スコープ: user（`claude mcp add -s user` と同位置）

プレースホルダ一覧は `sync.ps1` 参照。

## 追加/削除

1. `mcp-servers.json` を編集
2. 次回 PS 起動で自動反映 → Claude Code 再起動で認識

`claude mcp add` は使わない（sync の差分判定と競合）。`~/.claude.json` 直編集も次回 sync で上書き。
