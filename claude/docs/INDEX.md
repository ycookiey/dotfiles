# 補助 docs index

CLAUDE.md「参照ドキュメント」節の主要 docs と異なり起動前読込は不要。
症状・トピックに当たった時の grep 起点として使う。

- Cursor Agent CLI(`agent.cmd` 罠) → cursor.md
- ローカル LLM(llama.cpp 運用・モデル選定・VRAM/context 上限) → local-llm.md
- ローカル port 予約表 → ports.md
- Scoop + PortableApps wrapper 起動 → scoop-portable.md
- Claude Code Task 機能 → task.md
- Claude Code 内部挙動の調べ方(agent 定義の frontmatter の項目名・効かない項目、statusline の実入力採取、auto-compact 閾値の式、バイナリからの実装抽出、transcript での実測、公式 docs の仕様は WebFetch 要約でなく .md 原文で確認、settings 変更の反映確認) → claude-code-internals.md
- Claude in Chrome の操作が効かない(送信無反応・blur 不発は hidden タブ、古い ref、非 ASCII 入力)、hidden タブで遅延読み込み・タイマーが止まる、長い結果の回収(返り値の上限、get_page_text の空白) → claude-in-chrome-hidden-tab.md
- drizzle(SQLite / D1 の migration のテーブル作り直しで列欠落・子行消失、D1 の batch に `db.run` を入れられない) → drizzle.md
- pnpm(Windows で `link:` の区切りがバックスラッシュになる、新しい worktree の `pnpm install` が better-sqlite3 の C++ のビルド(ClangCL・MSB8020)で止まる、`link:` の package の peer が link 先の copy に解決され型が合わない、link 先を build し直すと Vite の dev server が消えた dist を参照して 500) → pnpm.md
- React(React Compiler lint の `react-hooks/refs`: ref を含むオブジェクトを props に渡すとエラー) → react.md
- WezTerm の Lua 設定(評価時に始めた `call_after` が動かない、`CloseCurrentPane` がアクティブなペインを閉じる、前面プロセスが子孫・短命なプロセスを返す、Claude Code 更新時の `claude.exe.old.*` 改名、spawn/split の cwd・args が使われない、nushell の cd と OSC 7、`send_text` の取りこぼし、別プロセスでのテスト) → wezterm.md
- WSL の memory(file cache が上限まで溜まり Windows を圧迫・即時に返す・上限の目安)、暇で止まると /tmp が消える → wsl.md
- 自分の CLAUDE.md・skill・agent の prompt を監査・書き換えるときのユーザー方針(自発性の文言は残す・global CLAUDE.md は短く) → prompt-writing.md
