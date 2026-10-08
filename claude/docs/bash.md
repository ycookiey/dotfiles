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

## grep 終了コードを誤解しない

- exit: `0`=マッチ / `1`=非マッチ / **`≥2`=エラー**(クラッシュは 128+signal。SIGABRT=134)
- `grep -q … || fallback` は `≥2` も fallback に流す → 異常終了を「非マッチ」と誤判定し誤動作(例: 乖離検知が全項目を偽の「未掲載」に)
- エラーと非マッチを区別:

```bash
grep -qiF -- "$pat" "$f"; case $? in 0) 一致 ;; 1) 不一致 ;; *) エラー処理 ;; esac
```

## MSYS パス変換が `/X` 形式オプションを破壊

- Git Bash は `/` 始まりの引数を POSIX→Windows パスへ自動変換 → `iscc /DName=val` 等の Windows 形式オプションが壊れる(例: ISCC「You may not specify more than one script filename」)
- 回避: `MSYS2_ARG_CONV_EXCL="/D" cmd ...`(複数prefixは `;` 区切り、全除外は `*`)
- 全除外時は引数内のパスを自前で Windows 形式に(`cygpath -w`)
- 例: `cmd.exe /c <command>` の `/c` が `C:/` に変わり、cmd が対話の画面で起動して入力を待ち止まる → `cmd.exe //c ...`。hook `cmd-switch-guard.sh` が `/c` の形を止める

## dev server は PowerShell の tool で起動する(Git Bash の run_in_background だと TaskStop 後も残る)

- 現象: Git Bash の run_in_background で `pnpm dev` 等を起動 → TaskStop 後も node(vite)と、Cloudflare Vite plugin なら `workerd.exe` が残り port を掴み続ける。`--strictPort` の再起動が失敗、無指定なら別 port にずれる
- 原因: npm の global の shim(`AppData/Roaming/npm/pnpm`、sh スクリプト)を Git Bash が MSYS の fork/exec で起動する所で Windows の親子のつながりが切れる(根元の `sh.exe` の ParentProcessId の process がもうない)。TaskStop は木をたどって止めるので、切れた先に届かない。bash が `node` を直接起動した場合はつながり、全部止まる(実測)
- 対処: PowerShell の tool の run_in_background で起動(`Set-Location C:/path/app; pnpm dev --port <port> --strictPort *> <log>`)。claude.exe までつながり、TaskStop で workerd まで止まって port も空く(実測)。hook `devserver-launch-guard.sh` が、Bash の run_in_background での dev server の起動を止める
- 残ったものの片付け: `Get-NetTCPConnection -LocalPort <port> -State Listen` の OwningProcess の CommandLine を `Get-CimInstance Win32_Process -Filter "ProcessId=<pid>"` で確認し、自分が起動したものだけ止める。つながりが切れた木は、切れた所の根元(親のない `sh.exe`)を `taskkill /PID <pid> /T /F` で止めると workerd まで止まる
- 他の session の dev server が同じ PC で動いていることがある。CommandLine の path と port で自分のものか確かめてから止める

## Git Bash の `sed -i` が CRLF を LF に変える

- 現象: CRLF のファイルに `sed -i` → 置換していない行も含めて全行 LF になる (GNU sed 4.9 で実測)
- git 管理下 (autocrlf) は commit 時に正規化され差分にならない。問題になるのは CRLF 必須のファイル (.bat 等) と git 管理外のファイル
- 対処: CRLF のファイルは Edit ツールで直す。`sed -i` を使ったら `unix2dos -q <file>` で戻す。確認は `file <file>` の「with CRLF line terminators」

## ツール入力の `\uXXXX` は文字に復号される

- 症状: Bash の heredoc や Edit の文字列に書いた `\u0020` が、送信時に実際の文字(空白など)へ変わる。正規表現の `\uXXXX` エスケープをそのまま書けない(Edit は「old と new が同一」エラー、python は想定外の文字で置換)
- 対処: python で `chr(92) + "u0020"` のように組み立てて書き、`od -c` で確認する
- `\\` が `\` 1 本になる件(上の「`\\` が縮む」)も同時に当たる。python の文字列に `"\\n"` と書くと改行 1 文字になり、パッチの照合が失敗した。`javascript_tool` では `\d` `\/` `\s` `\w` `\]` はそのまま届いた

## Windows: http.server を配るフォルダの中から起動すると、そのフォルダを消せない

- 症状: `cd dir && python -m http.server` の実行中、`dir` を削除すると EPERM(プロセスの作業フォルダがロックする)。生成し直す処理(`rmSync(dir)` 等)が失敗する
- 対処: 別の場所から `python -m http.server <port> --directory <dir>` で配る

## Windows の python を Git Bash から使うときの食い違い

- `/tmp` が別の場所: Git Bash の `/tmp` = `%TEMP%`(`C:\Users\<user>\AppData\Local\Temp`)、Windows の python の `/tmp` = `C:\tmp`。受け渡しは Windows パス(`C:/...`、scratchpad 等)で書く
- text mode の書込は改行を CRLF に変える: `Path.write_text` / `open(p, "w")` は `\n` を `\r\n` で書く。LF のファイルを置換すると全行 CRLF になる(git の警告「CRLF will be replaced by LF」で気づく)。`read_bytes().decode()` → `write_bytes(s.encode())`、または `open(p, "w", newline="")` を使う。確認は `git ls-files --eol <path>` の `w/lf`
