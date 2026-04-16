---
name: commit
description: git-commit-info.ps1を使ったトークン効率の良いcommitワークフロー
allowed-tools: Bash, Read, Edit
---

# Commit

1. `git-commit-info` を実行（PATH上）
2. WARNING行があれば対処を確認
3. 現在のタスクに関連する変更のみcommit対象。無関係な変更は含めない
4. 対象が複数の論理単位にまたがる場合は分割commitを提案
5. 出力を基にcommitメッセージを生成（タイトル1行、英語、過去スタイルに合わせる）
   - `recent-style:` の統計に従いbodyの有無も合わせる
6. 対象ファイルのみgit addしてcommit

## 部分ステージング（1ファイル内の一部だけcommitしたい場合）

`git-lines` CLI を使用:

1. `git-lines diff` — 変更行を行番号付きで表示
2. `git-lines stage <file>:<lines>` — 指定行のみステージ
   - 複数行: `file.rs:10,15,20`
   - 範囲: `file.rs:10..20`
   - 削除行: `file.rs:-5`
   - 複数ファイル: `file1.rs:10 file2.rs:20`
3. ステージ後に通常通りcommit
