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

## 実行手順

1. ユーザーから依頼内容を受け取る
2. 対象ディレクトリを特定（カレントディレクトリまたはユーザー指定）
3. Gitの差分レビューなら `codex exec review` を、それ以外は `codex exec` を使う
4. プロンプト末尾に「確認不要」の指示を追加
5. コマンドを実行
6. 結果をユーザーに報告
