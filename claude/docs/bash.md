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

## run_in_background の dev server は TaskStop 後も node が残ることがある

- 現象: `pnpm dev` 等を run_in_background で起動 → TaskStop 後も node 子プロセスが port を掴み続け、次回起動が別 port にずれる
- 停止後の確認: `Get-NetTCPConnection -LocalPort <port> -State Listen` → OwningProcess の CommandLine を `Get-CimInstance Win32_Process -Filter "ProcessId=<pid>"` で確認 → 自分が起動したものだけ `Stop-Process -Id <pid> -Force`

## ツール入力の `\uXXXX` は文字に復号される

- 症状: Bash の heredoc や Edit の文字列に書いた `\u0020` が、送信時に実際の文字(空白など)へ変わる。正規表現の `\uXXXX` エスケープをそのまま書けない(Edit は「old と new が同一」エラー、python は想定外の文字で置換)
- 対処: python で `chr(92) + "u0020"` のように組み立てて書き、`od -c` で確認する
- `\\` が `\` 1 本になる件(上の「`\\` が縮む」)も同時に当たる。python の文字列に `"\\n"` と書くと改行 1 文字になり、パッチの照合が失敗した。`javascript_tool` では `\d` `\/` `\s` `\w` `\]` はそのまま届いた

## Windows: http.server を配るフォルダの中から起動すると、そのフォルダを消せない

- 症状: `cd dir && python -m http.server` の実行中、`dir` を削除すると EPERM(プロセスの作業フォルダがロックする)。生成し直す処理(`rmSync(dir)` 等)が失敗する
- 対処: 別の場所から `python -m http.server <port> --directory <dir>` で配る
