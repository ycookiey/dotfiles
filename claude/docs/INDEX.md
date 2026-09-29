# 補助 docs index

CLAUDE.md「参照ドキュメント」節の主要 docs と異なり起動前読込は不要。
症状・トピックに当たった時の grep 起点として使う。

- Cursor Agent CLI(`agent.cmd` 罠) → cursor.md
- ローカル LLM(llama.cpp 運用・モデル選定・VRAM/context 上限) → local-llm.md
- ローカル port 予約表 → ports.md
- Scoop + PortableApps wrapper 起動 → scoop-portable.md
- Claude Code Task 機能 → task.md
- Claude in Chrome の操作が効かない(送信無反応・blur 不発は hidden タブ、古い ref、非 ASCII 入力)、hidden タブで遅延読み込み・タイマーが止まる、長い結果の回収(返り値の上限、get_page_text の空白) → claude-in-chrome-hidden-tab.md
- drizzle(SQLite / D1 の migration のテーブル作り直しで列欠落・子行消失、D1 の batch に `db.run` を入れられない) → drizzle.md
- pnpm(Windows で `link:` の区切りがバックスラッシュになる) → pnpm.md
