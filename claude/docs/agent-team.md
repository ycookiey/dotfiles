# Agent Team: Leadオーケストレーションガイド

## 基本原則

- **常に委譲する**: タスクが1つでもmemberに委譲する。Leadは実装・調査・レビュー・テストを自分でやらない
- **Leadの役割**: 委譲・統合・判断のみ。Leadのcontextを最小に保つ
- **並列化**: 独立したタスクは複数memberを同時起動する
- **Leadが直接実装・修正することは禁止。** memberが失敗してもLeadは委譲し続ける

## 起動手順

タスク数に関わらず必ずこの手順を踏む。1タスクでも省略しない。

1. `TeamCreate` でチームを作成
2. `TaskCreate` で全タスクを登録（依存関係は `blockedBy` で定義）
3. `Agent` で teammate を spawn（`team_name`, `name`, `subagent_type` を指定）
4. `TaskUpdate` で `owner` を設定してタスクを割り当てる
5. memberからの `SendMessage` を受けて統合・次の指示を出す
6. 完了後 `SendMessage` で `{type: "shutdown_request"}` を送りteamを終了

## Member起動判断

| 状況 | subagent_type |
|---|---|
| 設計・計画が曖昧・複雑 | planner（lead判断、毎回不要） |
| コードベース調査・外部情報収集が必要 | researcher |
| 実装・修正タスクがある | implementer（並列可） |
| 実装完了後 | reviewer |
| テストが必要 | tester |

## 典型的なフロー

```
シンプル:    implementer → reviewer（レビュー+テスト実行）
設計が必要:  planner → implementer → reviewer
調査が必要:  researcher → planner → implementer → reviewer
並列実装:    implementer×N → reviewer
TDD:         planner → tester（テスト先行作成）→ implementer（テストをパス）→ reviewer
```

**tester**: テスト作成のみ。実行はreviewerが行う。

## Contextの渡し方

memberはleadの会話履歴を持たない。spawn promptに以下を必ず含める:
- 絶対パス（相対パス不可）
- 前のmemberの出力サマリー
- 期待する出力形式
- 「何をしないか」の明示（例: レビューのみ、修正しない）

## Member起動モード

- **implementer / tester / reviewer**: `mode: "bypassPermissions"` で起動する。権限ブロックなしに実行できる
- **researcher / planner**: 読み取り専用のためmode指定不要

## Commitはimplementerの責任

Leadはcommitしない。implementerが自分の実装をcommitする。

## Memberがブロックした場合

memberは `SendMessage` でleadにブロッカーを報告する（何を試したか・何が原因か・何が必要か）。
Leadは `SendMessage` で追加情報を返すか、別memberに切り替える。

## タスク管理

- 完了したタスクも削除しない。進捗の記録として残す
- 依存関係は `blockedBy` で定義し、前タスク完了後に次を開始する

## 注意事項

- 同一ファイルを複数memberに同時編集させない
- token消費は起動数に比例（planner/reviewerはopusなので特に注意）
- teammateはターンごとにidle状態になるが正常動作。SendMessageで再起動する
