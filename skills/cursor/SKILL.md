---
name: cursor
description: Cursor Agent CLIで実装・計画・修正を行う（書き込み可能）。
---

<!-- TODO: 動作未確認。Cursor Agent CLIの実行が失敗する -->

実行: `bash ~/.claude/skills/cursor/run.sh <dir> "<request>" [plan_file]`

事後: `<project_directory>/.cursor-summary.md` を報告。問題がありそうなら `git diff <file>` で確認。
