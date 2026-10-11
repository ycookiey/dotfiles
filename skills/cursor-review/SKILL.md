---
name: cursor-review
description: Cursor Agent CLIでレビュー・分析・相談を行う（読み取り専用）。
---

# Cursor Review

Cursor Agent CLIを読み取り専用モード（ask）で実行し、レビュー・分析を行うスキル。

## コマンド

### 通常の分析・相談

```
agent --print --trust --mode=ask --workspace <project_directory> "<request>"
```

### Gitベースのコードレビュー

Cursor Agent CLIには `review` サブコマンドがないため、ask モードで差分を渡す:

```
# 未コミットの変更をレビュー
agent --print --trust --mode=ask --workspace <project_directory> "git diff の出力を確認し、以下の観点でレビューせよ: <instructions>"

# 特定ブランチとの差分をレビュー
agent --print --trust --mode=ask --workspace <project_directory> "git diff main...HEAD の出力を確認し、以下の観点でレビューせよ: <instructions>"
```

## プロンプトのルール

Cursor Agentに渡すリクエストの末尾に必ず以下を追加:

> 確認や質問は不要です。具体的な提案・修正案・コード例まで自主的に出力してください。
