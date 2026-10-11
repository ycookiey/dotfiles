# Claude Code 内部挙動の調べ方

## statusline の実入力を採取

- `<project>/.claude/settings.local.json` の `statusLine.command` を一時的に tee ラッパーへ差し替え (project 設定が user 設定より優先)
  - ラッパー: `in=$(cat); printf '%s\n' "$in" >> <scratchpad>/stdin.jsonl; printf '%s' "$in" | <本来のコマンド>`
- 差し替え前にバックアップ → 採取後はバックアップで復元
- 採取した JSON は修正前後の比較用 fixture に流用

## auto-compact 閾値 (v2.1.288 時点)

- 閾値 = window − min(maxOutput, 20000) − 13000 → 200k 窓: 167k (83.5%)、1M 窓: 967k (96.7%)
- 窓サイズ (window) は statusline 入力の `context_window_size` (200k / 1M)。比率の固定値 (0.835 等) は 200k 窓でしか合わない
- `CLAUDE_CODE_AUTO_COMPACT_WINDOW`: window の上限 (100k〜1M の範囲に丸める)
- `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`: 閾値を下げる方向のみ (min(有効窓×pct/100, 既定値))
- `DISABLE_AUTO_COMPACT` / `DISABLE_COMPACT`: auto-compact 無効化
- 環境変数から見えないもの: settings の `autoCompactWindow`、`/config` での auto-compact 切替、サーバ側の実験設定

## バイナリから実装を抽出

- `grep -a -o -E ".{300}<環境変数名や識別子>.{600}" ~/.local/bin/claude.exe`
- minify 後の関数名が別の処理と衝突する (同名の別関数がヒット)。周辺の文脈で同一性を確認してから結論する

## transcript で実測検証

- auto-compact の記録: jsonl の `compactMetadata` (`trigger:"auto"`, `preTokens`)
- 直前の assistant 行の `message.model` と照合して、窓サイズ別に集計
- 全 jsonl を python で走査すると 300 秒の timeout を超える → 先に `grep -rl '"trigger":"auto"'` で対象ファイルを絞る
- `subagents/agent-*.jsonl` はサブエージェントの記録。メインセッションの挙動と区別する
- 会話の記録は、会話を始めたフォルダの projects フォルダに置かれ続ける。会話の途中で作業フォルダが変わっても、記録は移らない (実測: repo A で始めて repo B に移った会話の記録が `C--Main-Project-A/` にあり、そこに追記され続けていた)
- hook の入力の `transcript_path` は、その時点の作業フォルダから組み立てたパスで、実在しないことがある (実測: 上の会話を repo B で `--resume` したとき、`…-B/<id>.jsonl` を受け取ったが、そのファイルは無かった)。記録の有無は `~/.claude*/projects/*/<id>.jsonl` を探して判定する
- 実行中の `claude.exe` は、自動更新で `claude.exe.old.<時刻>.<pid>` に改名される (実測)。プロセス名で Claude を見分けるときは前方一致にする

## 公式 docs で仕様を確かめる

- WebFetch は小モデルの要約。設定キーの型・既定値・必要 version を原文と逆に返すことあり (実例: settings-reference の `attribution`)
- 原文 Markdown: `curl -sL https://code.claude.com/docs/en/<page>.md -o <scratchpad>/<page>.md` → `grep -n <キー名>` → 該当節を `sed -n` で全部読む
- ページ一覧: `https://code.claude.com/docs/llms.txt`

## settings 変更の反映確認

- commit/PR の attribution はモデルが書く。settings の値は system-reminder の指示文として届く
- settings.json を編集すると同じセッション内で reminder が差し替わる (v2.1.289 で確認)。reminder の文面で反映を確かめる

## agent 定義の frontmatter

- 項目名は docs (sub-agents.md「Supported frontmatter fields」) の表と完全一致 (camelCase)。未知の項目はエラーなしで無視
- 権限は `permissionMode` (`mode:` は効かない)。未指定・親が bypassPermissions/acceptEdits/auto なら親のモードを継承
- `color` は red/blue/green/yellow/purple/orange/pink/cyan のみ

## plugin (mod) の実測 (v2.1.296)

- `claude -p --settings '{"disableAllHooks":true}'` では plugin の hooks module も動かず、mod の tool が登録されない(外すと登録。推定)
- `$` を渡せるのはファイル最上位で宣言した関数のみ(`claude plugin validate` が拒否)。`$` の型は `EngineInterface`(`Engine` は test 用)。型検査は types の先頭の tsconfig + tsc
- `$.ui.status`: 入力欄の下・statusline の上に 1 行。先頭に `⚠<plugin名>:` が付く。改行は 1 行につながり(◆)、ANSI は文字のまま。`undefined` で行ごと消える
- `ui.render` の AbovePrompt: 入力欄の真上に 1 行と右端の `[-]`。`Text` の `color`(theme key・16 進・名前)が効かず白
- 会話の開始後に install した plugin も `/reload-plugins` で読み込まれる(再起動不要)
- directory marketplace の plugin も、install 時に `~/.claude/plugins/cache/<marketplace>/<name>/<version>` へ写して読む。元の編集が反映されるかは要確認(公式 docs は「直接読む」と記載)
