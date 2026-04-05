---
name: commit
description: git-commit-info.ps1を使ったトークン効率の良いcommitワークフロー
allowed-tools: Bash, Read, Edit
---

# Commit

1. `pwsh -NoProfile -Command "& git-commit-info.ps1"` を実行（PATH上）
2. WARNING行があれば対処を確認
3. 現在のタスクに関連する変更のみcommit対象。無関係な変更は含めない
4. 対象が複数の論理単位にまたがる場合は分割commitを提案
5. 出力を基にcommitメッセージを生成（タイトル1行、英語、過去スタイルに合わせる）
6. 対象ファイルのみgit addしてcommit
