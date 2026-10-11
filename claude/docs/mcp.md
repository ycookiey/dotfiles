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

公開したくない MCP(対人の会話を扱うもの等)や特定プロジェクトだけで使う MCP は、`mcp-servers.json`(公開の dotfiles)ではなく、そのプロジェクトの `.mcp.json` に書く。sync の対象外なので消えない。初回に使用の承認が要る。

## claude.ai コネクタの罠

### Google Calendar: 終日予定が 1 日前にずれる

- `create_event` / `update_event` で `allDay: true` + `startTime: 2026-10-07T00:00:00+09:00` → UTC 変換後の日付(前日)で登録される(実測 2026-10)
- 回避: 日付を UTC の 0 時で渡す。`startTime: 2026-10-07T00:00:00Z` / `endTime: 2026-10-08T00:00:00Z`(end は翌日)
- 作成後は返り値の `start.date` で日付を確認する

### Google Calendar: 繰り返し予定の本体を更新すると、個別に変えた回の同じ項目も上書きされる

- 本体(`recurringEventId` の ID)で更新した項目は、以前に個別編集した回(`<id>_<日時>`)でも本体の値になる。更新しなかった項目は個別の値のまま(実測 2026-10: `location` を更新 → 個別の回の場所も上書き、別の日に `description` を更新 → 個別の回のメモも上書き。どちらも `summary` は残った)
- 回避: 本体を更新する前に、個別編集した回を `list_events` で控え、更新後に同じ項目を回ごとに戻して確認する。削除した回は削除のまま残る
