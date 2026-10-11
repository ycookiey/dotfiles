---
name: codex-review
description: Codex CLIでレビュー・分析・相談を行う（読み取り専用）。
---

# Codex Review

Codex CLIを読み取り専用モードで実行し、レビュー・分析を行うスキル。

## コマンド

### 通常の分析・相談

```
codex exec --full-auto --sandbox read-only --cd <project_directory> "<request>"
```

### Gitベースのコードレビュー

差分ベースのレビューには専用サブコマンドを使う:

```
# 未コミットの変更をレビュー
codex exec review --uncommitted --full-auto --cd <project_directory> "<instructions>"

# 特定ブランチとの差分をレビュー
codex exec review --base main --full-auto --cd <project_directory> "<instructions>"

# 特定コミットをレビュー
codex exec review --commit <sha> --full-auto --cd <project_directory> "<instructions>"
```

## プロンプトのルール

codexに渡すリクエストの末尾に必ず以下を追加:

> 確認や質問は不要です。具体的な提案・修正案・コード例まで自主的に出力してください。
