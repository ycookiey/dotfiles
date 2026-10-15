# Bash / sh スクリプト規約 (Git Bash on Windows)

実行シェルは Git Bash。Claude Code hook の bash も同環境。

## grep 3.0 ロケールクラッシュ (重要)

- Git Bash同梱 GNU grep 3.0 は **Cロケール/未設定 + `-i`** で SIGABRT(exit 134)。ファイル内容不問(ASCIIでも落ちる)
- Claude Code の **hook bash はロケール未継承(C相当)起動** → `grep -i` スクリプトが hook 経由で全クラッシュ。手動実行(`LANG=ja_JP.UTF-8`等)では再現せず気づきにくい
- 回避: 冒頭で UTF-8ロケール確保。判定に `grep -i` を使わない(鶏卵回避で case 照合):

```bash
case $'\n'"$(locale -a 2>/dev/null)"$'\n' in
  *$'\nC.utf8\n'*|*$'\nC.UTF-8\n'*) export LC_ALL=C.UTF-8 ;;
  *) case "${LANG:-}" in *[Uu][Tt][Ff]*) export LC_ALL="$LANG" ;; esac ;;
esac
```

## multi-byte 文字クラスの byte 化 (silent false positive)

- Cロケールでは `[αβγδεζ]` 等の multi-byte char class が UTF-8 バイト列 (CE B1..B6) から成る byte class に degrade → 日本語テキストのバイトに誤マッチ
- SIGABRT ではなく silent な誤検知。hook などバックエンドで走る script では発火するまで気付かない
- 回避は上と同じく冒頭で UTF-8 locale 強制

## 確認に使う変数が空だと、もっともらしい誤った結果が出る

- 例: 値を抜く `grep -oP` が locale で失敗 → 変数が空のまま `git log -S"$tok"` = `-S""` が全 commit に当たり、それらしい件数が出た
- 確認 script では抜いた直後に空で止める: `: "${tok:?empty}"`(`set -u` は空文字を止めない)

## grep 終了コードを誤解しない

- exit: `0`=マッチ / `1`=非マッチ / **`≥2`=エラー**(クラッシュは 128+signal。SIGABRT=134)
- `grep -q … || fallback` は `≥2` も fallback に流す → 異常終了を「非マッチ」と誤判定し誤動作(例: 乖離検知が全項目を偽の「未掲載」に)
- エラーと非マッチを区別:

```bash
grep -qiF -- "$pat" "$f"; case $? in 0) 一致 ;; 1) 不一致 ;; *) エラー処理 ;; esac
```

## `grep -v` で除外すると、見たい行も一緒に消える

- 除外語は見たい event の行にも載りうる。実例: 診断出力を `grep -v "Mini Krug"` で絞ったら、`monster_reset Mini Krug` の行も消え、既にあった reset を見落とした
- 除外する前に、見たい種類の行を肯定で抜き出して数を確かめる(`grep -E 'reset|error' | wc -l`)。除外はその後に、表示を減らす目的だけで使う

## MSYS パス変換が `/X` 形式オプションを破壊

- Git Bash は `/` 始まりの引数を POSIX→Windows パスへ自動変換 → `iscc /DName=val` 等の Windows 形式オプションが壊れる(例: ISCC「You may not specify more than one script filename」)
- 回避: `MSYS2_ARG_CONV_EXCL="/D" cmd ...`(複数prefixは `;` 区切り、全除外は `*`)
- 全除外時は引数内のパスを自前で Windows 形式に(`cygpath -w`)
- 例: `cmd.exe /c <command>` の `/c` が `C:/` に変わり、cmd が対話の画面で起動して入力を待ち止まる → `cmd.exe //c ...`。hook(yhooky の規則 `cmd-switch-guard`)が `/c` の形を止める
- 例: `AutoHotkey64.exe /ErrorStdOut /validate x.ahk` の `/ErrorStdOut` が `C:/.../git/<版>/ErrorStdOut` になり、AutoHotkey はそれをスクリプトとして開こうとして、ユーザーの画面に「Script file not found」のダイアログを出す(stderr には何も出ない)。GUI のプログラムは、こうして実行の失敗がユーザーの画面に出る。hook(yhooky の規則 `exe-switch-guard`)が、`.exe` か、コマンドの位置の `$VAR` に `/英字` を渡す形を止める
- 書き換わる形(Git for Windows 2.53 で実測): `/Foo` と `/Foo=UTF-8` は `C:/.../git/<版>/Foo...` になり、引用符で囲んだ `"/Foo"` も同じ。1 文字の `/S` は `S:/` になる。書き換わらない形: `//Foo` と、`:` を含む `/Foo:x`。`MSYS_NO_PATHCONV=1` も `MSYS2_ARG_CONV_EXCL='*'` と同じく変換を止める

## Claude Code の Bash ツールで `\\` が縮む・長い heredoc が壊れる

- Bash ツールに渡したコマンド文字列は、bash が解釈する前に `\\` が `\` 1 本になる。`\d` や `\M` のような単独のバックスラッシュは残る
- シングルクォート内も `<<'EOF'` の heredoc 内も同じ。bash の仕様では変換されない場所なので、bash 側の問題と誤認しやすい
- 実測: `printf '%s\n' 'a\\b'` の出力が `a\b`。heredoc の行末の `\\` が `\` になって次の行とつながり、終端の `EOF` を読み飛ばした
- クォートを多く含む数百行の heredoc(Python スクリプト等)は `unexpected EOF while looking for matching` で構文解析に失敗し、何も実行されないことがある
- 回避: バックスラッシュや複雑なクォートを含む内容は Write ツールでファイルに書いてから Bash で実行する。スクリプト内の Windows パスは `C:/path` のようにスラッシュで書く
- printf の書式文字列に Windows パスを書かない。`C:\Main\Project\twitchReader\venv` の `\t` `\v` がタブ・垂直タブになりパスが壊れる(実測: 生成した bat が「filename, directory name, or volume label syntax is incorrect」で exit 1)。`printf '%s' "..."` で引数に渡すか Write ツールで書く
- 改行コードの判定に `grep -c $'\r'` を使わない。Bash ツール経由では LF のファイルでも総行数が返り、全ファイルを CRLF と誤判定した。`file <path>` の「with CRLF line terminators」表示で判定する

## dev server は PowerShell の tool で起動する(Git Bash の run_in_background だと TaskStop 後も残る)

- 現象: Git Bash の run_in_background で `pnpm dev` 等を起動 → TaskStop 後も node(vite)と、Cloudflare Vite plugin なら `workerd.exe` が残り port を掴み続ける。`--strictPort` の再起動が失敗、無指定なら別 port にずれる
- 原因: npm の global の shim(`AppData/Roaming/npm/pnpm`、sh スクリプト)を Git Bash が MSYS の fork/exec で起動する所で Windows の親子のつながりが切れる(根元の `sh.exe` の ParentProcessId の process がもうない)。TaskStop は木をたどって止めるので、切れた先に届かない。bash が `node` を直接起動した場合はつながり、全部止まる(実測)
- 対処: PowerShell の tool の run_in_background で起動(`Set-Location C:/path/app; pnpm dev --port <port> --strictPort *> <log>`)。claude.exe までつながり、TaskStop で workerd まで止まって port も空く(実測)。hook(yhooky の規則 `devserver-launch-guard`)が、Bash の run_in_background での dev server の起動を止める
- 残ったものの片付け: `Get-NetTCPConnection -LocalPort <port> -State Listen` の OwningProcess の CommandLine を `Get-CimInstance Win32_Process -Filter "ProcessId=<pid>"` で確認し、自分が起動したものだけ止める。つながりが切れた木は、切れた所の根元(親のない `sh.exe`)を `taskkill /PID <pid> /T /F` で止めると workerd まで止まる
- 他の session の dev server が同じ PC で動いていることがある。CommandLine の path と port で自分のものか確かめてから止める

## Git Bash の `sed -i` が CRLF を LF に変える

- 現象: CRLF のファイルに `sed -i` → 置換していない行も含めて全行 LF になる (GNU sed 4.9 で実測)
- git 管理下 (autocrlf) は commit 時に正規化され差分にならない。問題になるのは CRLF 必須のファイル (.bat 等) と git 管理外のファイル
- 対処: CRLF のファイルは Edit ツールで直す。`sed -i` を使ったら `unix2dos -q <file>` で戻す。確認は `file <file>` の「with CRLF line terminators」

## autocrlf の作業ツリーは CRLF: スクリプトで照合する前に改行をそろえる

- 現象: `core.autocrlf=true` の repo の作業ツリー(とそこから cp したファイル)は CRLF。node / python で `'...\n...'` を `includes` / `replace` すると一致せず、置換が空振りする(実例: 衝突解消・コピーへのパッチが 2 回外れた)。Write で書いたファイルは LF なので、同じフォルダに CRLF と LF が混ざる
- 対処: 読んだ直後に `\r\n` → `\n` にそろえてから照合し、書き戻す改行を決める。照合が外れたら、まず `file <path>` で CRLF か確かめる

## ツール入力の `\uXXXX` は文字に復号される

- 症状: Bash の heredoc や Edit の文字列に書いた `\u0020` が、送信時に実際の文字(空白など)へ変わる。正規表現の `\uXXXX` エスケープをそのまま書けない(Edit は「old と new が同一」エラー、python は想定外の文字で置換)
- 対処: python で `chr(92) + "u0020"` のように組み立てて書き、`od -c` で確認する
- `\\` が `\` 1 本になる件(上の「`\\` が縮む」)も同時に当たる。python の文字列に `"\\n"` と書くと改行 1 文字になり、パッチの照合が失敗した。`javascript_tool` では `\d` `\/` `\s` `\w` `\]` はそのまま届いた

## Windows: http.server を配るフォルダの中から起動すると、そのフォルダを消せない

- 症状: `cd dir && python -m http.server` の実行中、`dir` を削除すると EPERM(プロセスの作業フォルダがロックする)。生成し直す処理(`rmSync(dir)` 等)が失敗する
- 対処: 別の場所から `python -m http.server <port> --directory <dir>` で配る

## `wsl.exe -- bash -c '...$VAR...'` が何も出さずに失敗する

- 現象: Git Bash から `MSYS_NO_PATHCONV=1 wsl.exe -d <distro> -- bash -c '<$ を含むコマンド>'` が出力なしの exit 2 / 1。`$` を含まないコマンドは通る(2026-10 実測、原因は未確認)
- 回避: コマンドを WSL 側のファイル(`//wsl.localhost/<distro>/home/<user>/x.sh`)に書き、`wsl.exe -d <distro> -- bash /home/<user>/x.sh` で実行する
- WSL の出力を `| grep` などに通すと、終わるまで溜まって流れないことがある。結果は WSL 側のファイルに書いて読む

## Windows の python を Git Bash から使うときの食い違い

- `/tmp` が別の場所: Git Bash の `/tmp` = `%TEMP%`(`C:\Users\<user>\AppData\Local\Temp`)、Windows の python・node の `/tmp` = `C:\tmp`。受け渡しは Windows パス(`C:/...`、scratchpad 等)で書く。yhooky の規則 `tmp-path-reminder` が、node・python と `/tmp/` を含むコマンドの直前に差し込む
- text mode の書込は改行を CRLF に変える: `Path.write_text` / `open(p, "w")` は `\n` を `\r\n` で書く。LF のファイルを置換すると全行 CRLF になる(git の警告「CRLF will be replaced by LF」で気づく)。`read_bytes().decode()` → `write_bytes(s.encode())`、または `open(p, "w", newline="")` を使う。確認は `git ls-files --eol <path>` の `w/lf`
- 作業ツリーの file は autocrlf で CRLF のことがある(git の中の版は LF)。Python で `"…\n"` の目印を探すと、作業ツリーでは見つからず、HEAD の版では見つかる、と食い違う。目印は改行を検出して(`"\r\n" in t`)合わせる。実例: 作業ツリーの plan.md(CRLF)で `assert t.count(anchor) == 1` が止まり、先に `open(p, "wb")` で開いていたので file が空になった(yhooky の規則 `truncating-write-reminder`)。既存の file の一部を直すだけなら Edit ツールを使う(改行を保つ)
- `print` も stdout に `\r\n` を書く(Windows の `jq -r` の出力も同じ): bash の `while read a b` で読むと最後の変数の末尾に `\r` が残る(ファイル名なら ffmpeg が「出力の形式を選べない」で失敗)。読む側で `tr -d '\r' <file | while read …`、出す側で `sys.stdout.reconfigure(newline="\n")`

- `python -I` は user site(`pip install --user`、Windows では `AppData/Roaming/Python`)も無効にするので、そこに入れた numpy 等が `ModuleNotFoundError` になる。スクリプトのフォルダや cwd を `sys.path` に入れない目的なら `python -E -P -B`(3.11+)で足り、user site は残る(実測 3.14)。`-B` が要るのは `-E` が `PYTHONDONTWRITEBYTECODE` も無視し、スクリプトの横に `__pycache__` を作るため。hook(yhooky の規則 `python-isolated-reminder`)が `python -I` の直前に差し込む

## ファイルを変数で渡すコマンドは `"$f"` と引用する

- 引用しない `$f` が空だと grep・sed・cat は引数なしになり、標準入力を待って止まる(Bash ツールでは時間切れまで返らない。`f=$(… | head -1)` が何も返さなかったときに起きた)
- 引用すれば「ファイルが無い」で即座に失敗する。入力が要らないコマンドには `</dev/null` も付ける

## 長い計測を background で回す間に import 元を編集しない

background の計測 (tsx / node / python の script) は、後から起動するプロセスや遅延 import がその時点の working tree を読む。計測中に src を編集すると、結果が新旧混在になる (前半と後半で別の code)。

- 対策: 計測を `git worktree add <tmp> HEAD` の固定版で回す、または計測が終わるまで import 元を編集しない
- 結果を使う前に、計測開始後に import 元を編集・commit していないか確かめる。していれば測り直す
