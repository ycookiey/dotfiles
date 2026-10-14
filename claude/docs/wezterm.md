# WezTerm の Lua 設定の罠

nightly 20251201 / Windows で実測。

## 設定の評価時に始めたタイマーが動かない

- 観測: 設定の自動再読込 1 回で、設定ファイルの評価が 2 回走った (ログ上で 0.2 秒差)
- 実測: 評価時に世代番号を `wezterm.GLOBAL` に書き、`wezterm.time.call_after` で定期処理を始めた。すると後の評価の世代番号が残り、その世代のループは一度も発火しなかった。先の世代のループは、世代が違うため止まった
- 原因は要確認 (後の評価の Lua 状態が使われずに捨てられている、と推定)
- 対処: 定期処理は評価時ではなく、イベント (`window-config-reloaded` など) の中から始める
  - モジュール内のフラグで、1 回に絞る
  - 古いループが残っても重複しないよう、`wezterm.GLOBAL` の世代番号で古いループを止め、前回実行時刻で間引く
  - 実測: この形で、ウィンドウを開いたときと再読込のたびに 1 本だけ動いた

## `CloseCurrentPane` は渡したペインを閉じない

- `window:perform_action(act.CloseCurrentPane{...}, pane)` は、引数の `pane` ではなく、その窓でアクティブなペインを閉じる
- 対処: 閉じたいペインの `pane:tab():activate()` と `pane:activate()` を先に呼ぶ

## プロセス情報

- `pane:get_foreground_process_name()` / `get_foreground_process_info()` は、そのペインで最も深い子孫を返すことがある。Claude Code のペインでは claude.exe ではなく、その子の node.exe や python.exe (MCP サーバー等) が返った。特定のプログラムが動いているかは、`wezterm.procinfo.get_info_for_pid(pid).ppid` で親をたどって判定する
- 前面のプロセスは短命なもの (Claude Code のステータス行で起動するコマンド等) のことがある。その pid を `get_info_for_pid` で問い合わせ直すと、既に終了していて nil になり、親をたどれなかった (実測)。最初の一歩は `get_foreground_process_info()` が返した情報 (`ppid` を含む) をそのまま使う
- Claude Code の自動更新は、実行中の `claude.exe` を `claude.exe.old.<時刻>.<pid>` に改名する (実測)。プロセス名を `claude.exe` との完全一致で判定すると、更新後に見つけられなくなる。前方一致で判定する
- 前面のプロセスの情報は古いことがある。シェルに打ち込んで起動した claude が、画面に出力の無い間 (フォルダの信頼の確認画面で止まっている間) は見えず、前面がシェルのままだった (実測)。親をたどって見つからなければ、`get_info_for_pid(fg.pid).children` (問い合わせ直すと最新の子が入る) を浅い順に探す
- `wezterm.procinfo.get_info_for_pid` は、終了したプロセスに対して nil を返す。プロセスが生きているかの判定に使える
- 自分自身の GUI プロセスの `argv` は空で返る (`wezterm start -- prog` で起動した場合も)。起動引数で窓の用途を見分けることはできない
- シェルが入力待ちかを、前面のプロセスがシェルかどうかで判定すると取りこぼす。nushell のペインで、コマンドの実行直後 (次のプロンプトを描く間) に前面が `git.exe` になっていた (実測。プロンプトの表示で git を呼ぶため)。「Claude が動いていない」のように、除外したいものが動いていないことで判定する

## ペインの配置

- `tab:panes_with_info()` は、ズーム中でも全ペインを元の位置と大きさ (left / top / width / height) で返す (`is_zoomed` が true になるだけ)
- ペイン間の境界線は 1 セル分ある。左右に並ぶペインでは、右のペインの `left` が左のペインの `left + width + 1` になる

## spawn / split の指定 (`cwd`・`args`) が使われないことがある

- 実測 (Lua の `mux_window:spawn_tab{cwd, args}` / `pane:split{cwd, args, direction}`、ペインを続けて 5〜9 個作る処理):
  - `cwd` が無視され、分割元・操作したペインのフォルダや nu.exe のインストール先で開いた。起きる位置は回ごとに違い、`args` の有無にかかわらず起きた
  - 9 ペイン中 1 つで、`cwd` と `args` が丸ごと使われず、`default_prog` が起動コマンドなしで開いた
  - 単独の実験 (`args` を指定した `spawn_tab`) では、3 回中 3 回 `cwd` が無視された
- `wezterm cli split-pane` でも、繰り返し実行したときに空のペイン ID が返る失敗があり、Lua より確実とは言えない
- 原因は要確認
- 対処:
  - フォルダは、起動コマンドの先頭で移動する (nushell なら `cd r#'<path>'#; ...`。raw 文字列なので、パスに引用符が入っても壊れない)
  - 起動コマンドが落ちたことに気づけるよう、後から結果を確かめる (例: 再開するはずのプログラムが動いているかを見て、動いていなければ手で戻すコマンドを知らせる)

## nushell の `cd` は端末に伝わらない (Windows の既定)

- 観測: `nu -e "cd X"` で開いたペインは、プロンプトでは X にいるのに、WezTerm の `pane:get_current_working_dir()` は起動時のフォルダのままだった
- nushell の `shell_integration.osc7` は Windows では既定で false。WezTerm はその場合プロセスの作業フォルダを見るが、nushell は `cd` してもそれをすぐには変えないと推定 (要確認)
- 対処: nushell の設定で `$env.config.shell_integration.osc7 = true`。有効にすると、`cd` のたびに WezTerm 側のフォルダも追従した (実測)

## 分割を作ってから窓を広げると比率が崩れる

- 観測: 80 桁の窓で 5 ペインを比率どおりに分割してから最大化すると、配置が元の比率から崩れた
- 対処: 先に窓を目標の大きさにし、`window-resized` イベントを受けてから分割する
  - タイマーでは待たない
  - 実測: 最大化してから分割すると、元の 118/119/119/119 桁に対して 119/121/117/118 桁になった (比率を整数の桁に丸めるため 1〜2 桁ずれる)
- 2 枚目以降の窓 (`wezterm.mux.spawn_window{width, height}` で開く) の最大化は、GUI の窓ができてから行う。`window-config-reloaded` だけでは 2 枚中 1 枚を取りこぼした (実測) ため、`update-status` でも確かめる
- `update-status` と、非推奨の `update-right-status` は、WezTerm のソース (`emit_status_event`) で両方とも毎回出されている。片方に処理を登録しても、もう片方は止まらない
- 最大化の状態を返す API は無い (`window:get_dimensions()` は pixel_width / pixel_height / dpi / is_full_screen だけ)。最大化かどうかは、窓の大きさを `wezterm.gui.screens().by_name` の各モニターの width / height と比べて判定した。3840×1080 のモニターで最大化した窓は 3840×1080 ピクセルだった

## ペインを作った直後にシェルへ文字を送ると取りこぼす

- 観測: action_callback の中で新しいペインを作り、`call_after` で 0.5 秒以上待ってから `pane:send_text("cmd\r")` を送った。ペインには nushell のプロンプトが出たが、コマンドは実行されず、ログにもエラーが出なかった
- 原因は要確認。`call_after` が発火しなかったのか、文字がシェルの起動前に届いて失われたのかは切り分けていない
- 一方、ペインを作ってから 30 秒以上たち、シェルが入力待ちのときに `update-status` のイベントの中から `send_text` すると、コマンドは実行された (実測 2 回)
- 対処: 最初のコマンドは、ペインを作るときの `args` に入れる (例: `{ "nu", "-e", "cmd" }`)。`nu -e` はコマンドの実行後に対話シェルとして残る。後から打ち込むのは、やり直しのように時間がたってからに限る

## テストのときの注意

- 普段使いの GUI を止めずに試すには、`wezterm start --always-new-process` で別プロセスを起動し、`WEZTERM_UNIX_SOCKET` にそのプロセスのソケット (`~/.local/share/wezterm/gui-sock-<pid>`) を指定して `wezterm cli` で操作する
- 指定したソケットのプロセスがまだ起動していないと、`wezterm cli` は `wezterm-mux-server --daemonize` を勝手に起動する。テストの後に `wezterm-mux-server.exe` が残っていないか確かめる
- Claude Code の Bash ツールから `wezterm start` すると、`CLAUDE_CODE_CHILD_SESSION` などの環境変数を引き継ぐ。その中で起動した Claude Code は、画面に「Transcript saving is off」と出て、会話を記録しない (`~/.claude/projects` に `.jsonl` ができない)。普段の使い方を再現するには、`env -u CLAUDE_... ` で `CLAUDE*` / `ANTHROPIC*` の変数を外して起動する
- 書き換え中の設定は、普段の設定フォルダ (`~/.config/wezterm`) ではなく作業用のコピーに置き、`wezterm --config-file <コピー>\wezterm.lua start --always-new-process` で試す。普段の設定フォルダのファイルを保存するたびに、普段使いの GUI が設定を読み直すため
  - `--config-file` は `start` より前に書く (後ろに書くと `unexpected argument` で起動しない)
  - `--config-file` で別のフォルダの設定を読ませても、`require` は普段の設定フォルダのモジュールを読んだ (実測)。コピーのモジュールを読ませるには、コピーの `wezterm.lua` の先頭で `package.path = [[<コピーのフォルダ>\?.lua;]] .. package.path` とする
- キーで呼ぶ処理を `wezterm cli` から試すには、テスト用の設定に `user-var-changed` のハンドラを足し、ペインのシェルから OSC 1337 の SetUserVar を出させる。nushell なら `wezterm cli send-text` で次を送る: `print -n $"(ansi escape)]1337;SetUserVar=<名前>=(random chars | encode base64)(char bel)"` (値を毎回変える)
