# OSS upstream への投稿

upstream OSS (3rd-party repo) への issue/PR/コメント投稿は事前承認必須。draft 提示→ユーザ承認後に実行。

## 投稿前チェックリスト

- 重複 issue/PR 検索 (`gh issue list -R <repo> --state all --search "<keyword>"`)
- 1 issue 1 problem 原則 — 複数問題を 1 issue に混ぜない
- 未検証主張を排除 — 検証済み事実と推論を分離。推論は "appears to" / "looks like" 等で hedge
- repro steps は実行したもののみ — 未実行を tested-looking に書かない
- 既存パターン整合 — CHANGELOG / 過去 issue / 既存コードの処理スタイルに合わせる。新動作クラスは flag
- 装飾削減 — 絵文字・三段見出し・"Comprehensive solution" 系前置きを避ける
- maintainer 判断領域を尊重 — 修正方向の押し付けを避け、"Is this in scope?" 等 scope question 形式
- `Fixes #...` を安易に書かない — maintainer の選択肢を狭める
- 簡潔 — 10-30 行目安
- advisor を通す — 上記項目を第三者視点で再点検
