# Claude Code Task機能

## 概要

複雑な多段階作業を複数エージェント間で調整・追跡する仕組み。Agent Teamと組み合わせて使う。

## ストレージ

タスクは `~/.claude/tasks/` にJSONファイルとして永続化される。

```
~/.claude/tasks/
├── <UUID>/                # セッション自動生成のタスクリスト
│   ├── .lock              # 排他制御
│   ├── .highwatermark     # 連番管理（存在しない場合もある）
│   ├── 1.json
│   ├── 2.json
│   └── ...
├── my-project/            # CLAUDE_CODE_TASK_LIST_ID で命名したタスクリスト
│   └── ...
```

- UUIDディレクトリ: セッション起動時に自動生成。セッション終了後も残る
- 名前付きディレクトリ: `CLAUDE_CODE_TASK_LIST_ID=名前` で起動するとセッション間で共有可能

## JSONスキーマ

```json
{
  "id": "1",
  "subject": "タスク名",
  "description": "詳細な説明",
  "status": "completed",
  "blocks": ["2", "4"],
  "blockedBy": [],
  "activeForm": "現在の作業状態メモ（任意）"
}
```

| フィールド | 説明 |
|---|---|
| id | タスクリスト内の連番（文字列） |
| subject | タスク名 |
| description | 詳細 |
| status | `pending` / `in_progress` / `completed` |
| blocks | このタスク完了までブロックされるタスクID |
| blockedBy | このタスクの前提タスクID |
| activeForm | 作業状態メモ（任意） |

## ツール一覧

| ツール | 役割 |
|---|---|
| TaskCreate | タスク作成・割り当て |
| TaskGet | タスクIDから詳細取得 |
| TaskList | 全タスク一覧・状態表示 |
| TaskUpdate | ステータスや所有者の更新 |
| TaskStop | バックグラウンドタスクの停止 |
| TaskOutput | 非推奨（Readを使用） |

## ステータス遷移

```
pending → in_progress → completed
```

- `blockedBy`が空の`pending`タスク = 即開始可能
- `blockedBy`に値がある`pending`タスク = 依存タスク完了待ち

## Agent toolとの使い分け

| 観点 | Agent tool | Task系ツール |
|---|---|---|
| スコープ | 同一セッション内サブエージェント | 複数セッション・エージェント間 |
| 用途 | 短期的・結果を親が即使う | 並列長期作業・依存関係あり |
| 例 | ファイルレビュー→結果返却 | 3人が並列にPRレビュー |

## 有効化（Agent Teams）

```json
// settings.json
{
  "env": {
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1"
  }
}
```

## UI

- Interactive モード: `Ctrl+T` でタスクリスト表示切替
