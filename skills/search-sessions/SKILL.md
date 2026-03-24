---
name: search-sessions
description: Claude Code会話履歴を検索・閲覧する。過去の作業内容や判断経緯を振り返る際に使用。
---

# Search Sessions

過去のClaude Codeセッションを検索・閲覧するスキル。

スクリプト: `~/.claude/skills/search-sessions/history.py`

## 関数

### list_sessions

セッション一覧を取得。

```bash
python ~/.claude/skills/search-sessions/history.py list [--project NAME] [--days N]
```

| 引数 | デフォルト | 説明 |
|------|-----------|------|
| `--project` | 全プロジェクト | プロジェクト名の部分一致フィルタ |
| `--days` | 30 | 直近N日 |

返却: session_id, project, mtime, first_prompt, cwd, git_branch

### search

全セッションを横断キーワード検索。

```bash
python ~/.claude/skills/search-sessions/history.py search QUERY [--project NAME] [--days N] [--limit N] [--regex]
```

| 引数 | デフォルト | 説明 |
|------|-----------|------|
| `QUERY` | (必須) | 検索キーワード（部分一致、大文字小文字無視） |
| `--project` | 全プロジェクト | プロジェクト名の部分一致フィルタ |
| `--days` | なし | 直近N日 |
| `--limit` | 20 | 最大結果件数 |
| `--regex` | false | クエリを正規表現として扱う |

返却: session_id, project, role, timestamp, snippet（前後80文字のコンテキスト付き）

### read_session

特定セッションのメッセージを読む。

```bash
python ~/.claude/skills/search-sessions/history.py read SESSION_ID [--role ROLE] [--tail N] [--include-tools]
```

| 引数 | デフォルト | 説明 |
|------|-----------|------|
| `SESSION_ID` | (必須) | セッションUUID |
| `--role` | all | `user` / `assistant` / `all` |
| `--tail` | なし | 末尾N件のみ（コンテキスト保護用） |
| `--include-tools` | false | ツール呼び出し/結果を含める（`--role all` 時のみ有効） |

## 使い方

1. `list` または `search` でセッションを特定
2. `read` でそのセッションの内容を確認
3. 大きなセッションは `--tail` で末尾だけ読む

## 注意

- 出力はJSON形式
- セッションJSONLが大きい場合、`read --include-tools` は大量のデータを返す。必要な場合のみ使用
- `--include-tools` は `--role all` 以外ではエラー
