---
name: ytasky
description: 時間ブロック型スケジューラ ytasky を MCP 経由で操作する
---

# ytasky

MCP server (`mcp__ytasky__*`) 経由で操作。CLI は使わない。

トリガー: **ytask** / **ytasky**。単独「task」は Claude Code の TaskCreate と衝突するため避ける。「タスク」も曖昧なので ytasky 文脈では `ytask` と明示。

## データモデル

- **scheduled**: `date` (YYYY-MM-DD) 上に並ぶ。`category_id` / `duration_min` (15 分単位推奨) / `fixed_start` (HH:MM 任意) / `actual_start`/`actual_end` / `sort_order`
- **backlog**: 未スケジュール。`deadline` のみ。`schedule_backlog` で日に挿入
- **recurrence**: `pattern` = `daily`/`weekly`/`monthly`、`pattern_data` は `{"days":[1,3,5]}` など

## category (固定 9 種)

`work` / `study` / `sleep` / `meal` / `exercise` / `personal` / `break` / `commute` / `errand`

## 主要 tool

- scheduled: `list_tasks` / `add_task` / `edit_task` / `delete_task` / `start_task` / `done_task` / `move_task`
- backlog: `list_backlog` / `add_backlog` / `edit_backlog` / `delete_backlog` / `schedule_backlog` / `to_backlog`
- recurrence: `add_recurrence` / `edit_recurrence` / `delete_recurrence` / `list_recurrences`
- 集計: `report` (category 別・title 別の planned/actual 分)
- 参照: `list_categories` / `history`

各 tool の引数は MCP instructions 参照 (自動提供)。

## 注意

- `fixed_start` / `deadline` / `end_date` / `pattern_data` を削除するときは値 `"none"` を渡す
- `move_task` は同一日の 2 task の sort_order swap
- recurrence 追加/編集で pattern 変更すると未完了・未来の task が再生成される
- 未定義 category で add すると reject される
