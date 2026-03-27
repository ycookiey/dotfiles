---
name: commit
description: git-commit-info.ps1を使ったトークン効率の良いcommitワークフロー
allowed-tools: Bash, Read, Edit
---

# Commit

1. `pwsh -NoProfile -Command "& git-commit-info.ps1"` を実行（PATH上）
2. WARNING行があれば対処を確認
3. 変更を確認し、無関係な変更が混在していれば論理単位ごとに分割commitを提案
4. 出力を基にcommitメッセージを生成（タイトル1行、英語）
5. 必要なファイルをgit addしてcommit
