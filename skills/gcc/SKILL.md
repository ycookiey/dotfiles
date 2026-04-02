---
name: gcc
description: GLM Claude Codeで実装・計画・修正を行う（Agent SDK経由、マルチターン対応）。
---

## 実行

`bash ~/.claude/skills/gcc/run.sh <dir> "<request>" [plan_file]`

## セッション再開

`bash ~/.claude/skills/gcc/run.sh <dir> "<追加指示>" "" --resume <sessionId>`

前回のセッションIDは実行結果の `sessionId:` 行に出力される。

## 事後

`<dir>/.gcc-summary.md` を報告。問題がありそうなら `git diff <file>` で確認。
