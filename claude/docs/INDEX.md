# 補助 docs index

CLAUDE.md「参照ドキュメント」節の主要 docs と異なり起動前読込は不要。
症状・トピックに当たった時の grep 起点として使う。

- Cursor Agent CLI(`agent.cmd` 罠) → cursor.md
- ローカル LLM(llama.cpp 運用・モデル選定・VRAM/context 上限) → local-llm.md
- ローカル port 予約表 → ports.md
- Scoop + PortableApps wrapper 起動 → scoop-portable.md
- Claude Code Task 機能 → task.md
- Claude Code 内部挙動の調べ方(agent 定義の frontmatter の項目名・効かない項目、statusline の実入力採取、auto-compact 閾値の式、バイナリからの実装抽出、transcript での実測、公式 docs の仕様は WebFetch 要約でなく .md 原文で確認、settings 変更の反映確認、壊した作業ツリーの file を file-history の写しから戻す、表示方式の判別・fullscreen の自動停止) → claude-code-internals.md
- Commit 分割戦略(fixup / 新規 commit の要素別判定) → commit-strategy.md
- Claude Code の hook を足す・直す(yhooky の規則のファイル・test・組み込みの判定、Git Bash 経由の起動で親のないプロセスが一時停止のまま残る罠) → hooks.md
- Claude in Chrome の操作が効かない(送信無反応・blur 不発は hidden タブ、古い ref、非 ASCII 入力)、hidden タブで遅延読み込み・タイマーが止まる、長い結果の回収(返り値の上限、get_page_text の空白)、LLM Web UI の反復実行(model 確認・上限・長文添付・結果回収・複数リストや件数崩れ・clipboard 衝突)、複数タブの並列実行(Vivaldi タイル表示、フォーカス、別ウィンドウ不可)、窓が見えず screenshot が timeout(headless Chromium を CDP で直接操作して撮る。終了は Browser.close でないと子プロセスが残る) → claude-in-chrome-hidden-tab.md
- ブラウザ拡張機能(MV3)とWebサイトの組み合わせ(`sessions` のタブの URL・題名が `tabs` 権限なしで欠ける、Vivaldi は履歴ページの置き換え不可、未パッケージの再読み込みが要る変更、`externally_connectable` のポート指定・未導入の判定、Web ページから `_favicon` を読む、ID の固定、Vivaldi の画面の表記の確かめ方、scoop の自己更新アプリの版ずれ) → browser-extension.md
- YouTube 動画の内容把握(文字起こし取得) → youtube-transcript.md
- Neovim の外部コマンドが動かない・遅い(Windows の `exepath()` が 'shell' で sh script / .bat を返す) → nvim.md
- drizzle(SQLite / D1 の migration のテーブル作り直しで列欠落・子行消失、D1 の batch に `db.run` を入れられない、generate の meta JSON が Prettier 未整形) → drizzle.md
- pnpm(Windows で `link:` の区切りがバックスラッシュになる、新しい worktree の `pnpm install` が better-sqlite3 の C++ のビルド(ClangCL・MSB8020)で止まる、`link:` の package の peer が link 先の copy に解決され型が合わない、link 先を build し直すと Vite の dev server が消えた dist を参照して 500) → pnpm.md
- Playwright(ブラウザ revision 不一致で起動失敗、evaluate/content にタイムアウトが無く巡回が止まる、スマホ向けページの WebKit での確認: Chromium だけの CSS・lazy 画像が撮れない、aria-disabled のボタンを click() が押さない・jsdom で再現しないクリック中の DOM 入れ替わり) → playwright.md
- React(React Compiler lint の `react-hooks/refs`: ref を含むオブジェクトを props に渡すとエラー、`use client` 部品のレンダー中処理はサーバーでも走り `ImageData` 等が未定義、rAF の再生ループで経過時間が負) → react.md
- WezTerm の Lua 設定(評価時に始めた `call_after` が動かない、`CloseCurrentPane` がアクティブなペインを閉じる、前面プロセスが子孫・短命なプロセスを返す、Claude Code 更新時の `claude.exe.old.*` 改名、spawn/split の cwd・args が使われない、nushell の cd と OSC 7、`send_text` の取りこぼし、前面のプロセスでシェルの入力待ちを判定できない、別プロセス・作業用コピーの設定でのテスト(`--config-file` でも `require` は普段のフォルダを読む、設定 error でも show-keys は既定の設定で exit 0)) → wezterm.md
- 画像の中身の確かめ(透過 PNG を Read で見ると不透明度が無視され、見えない画素が見える) → images.md
- elkjs(ELK layered の入れ子 `INCLUDE_CHILDREN` で model order 系オプションが落ちる・効かない、座標基準、エッジ/ノードラベルの配置) → elkjs.md
- League of Legends のゲームデータ(地形 navgrid・jungle camp/個体の spawn 位置・WAD の読み方。画像から推定しない) → league-game-data.md
- ESLint(eslintrc の parser・plugin は共有の設定の場所から解決され root の版は効かない、typescript-eslint の `project` 列挙で heap 不足 → `projectService`、`tsconfig.json` 以外の名前の設定にしか入らない file) → eslint.md
- Node.js / tsx(`tsx -e` の top-level await、`zlib.zstdDecompressSync` の型、ES2017 target の BigInt literal、tsx の計測が重く出る、Rust→wasm の profile と速さの比べ方、CI の npm の版ずれで npm ci が lock を拒否、npm trusted publishing で OTP なしに CI から公開) → nodejs.md
- WSL の memory(file cache が上限まで溜まり Windows を圧迫・即時に返す・上限の目安)、暇で止まると /tmp が消える → wsl.md
- Windows のプロセス・窓(Explorer でファイルを選んで開く: `explorer /select` は初めてのフォルダで選ばない、ファイル使用中で更新・削除が失敗したら掴んでいるプロセスをアプリのログ・handle64 で特定する、裏の子プロセスが親のパイプを受け継ぎ読む側が待たされる、窓の手前への切り替えの制限と本人の入力が重なる試験) → windows-desktop.md
- 自分の CLAUDE.md・skill・agent の prompt を監査・書き換えるときのユーザー方針(自発性の文言は残す・global CLAUDE.md は短く) → prompt-writing.md
- Office ファイルの生成(pptxgenjs で文字が Arial になる・日本語のフォントが効かない、python-pptx でスライドを消すとセクションに番号が残る、日本語の docx で行間の指定が効かない) → office-files.md
