# Git の罠

## .git 再初期化で symlink・実行ビット消失 (Windows)

- 症状: 上流repo(テンプレ等)の `.git` を削除→`git init` し直すと、symlink(120000)は平文ファイル(中身=リンク先パス文字列)、実行ファイル(100755)は100644でcommitされる
- 検知: CIで `./script` が **exit 126**(permission denied)。symlinkだったファイルを cat するとパス文字列1行のみ
- 原因: Windows checkout は core.symlinks=false・core.filemode=false → 作業ツリーにmode情報なし、新indexは全て100644
- 復元(index直接操作。Windows FS上のファイルはそのままでよい):
  - 実行ビット: `git update-index --chmod=+x <file>`
  - symlink: `git update-index --cacheinfo "120000,<blob>,<path>"`(blob=リンク先パスの文字列。平文化したファイルの中身がリンク先パスそのものなら `git rev-parse :<path>` で流用可)
- 上流の正しいmode一覧: `gh api "repos/<owner>/<repo>/git/trees/HEAD?recursive=1" --jq '.tree[] | select(.mode!="100644" and .mode!="040000") | [.mode,.path] | @tsv'`
- 予防: 上流の履歴を切りたいだけなら `git checkout --orphan` や `git clone --depth 1` + 履歴squashの方がmode情報を保持できる

## rebase 中の手動 commit は rewritten-list に登録されない

- 症状: conflict 解決→`git commit`→`git rebase --continue` で「You must edit all merge conflicts」エラー。`git ls-files -u` 空・conflict marker 無しなのに詰む
- 原因: rebase の `pick` ステップは内部 `rewritten-list`(元 sha → 新 sha)に対応を登録して進む。手動 `git commit` は rebase 機構の外で commit を作るだけで未登録、git は「まだ pick 中」と判定し続ける
- 検知: `.git/rebase-merge/rewritten-list` に該当 sha の対応が無い / `.git/rebase-merge/stopped-sha` が元 commit を指したまま
- 通常運用: conflict 解決は `git add` → `git rebase --continue` のみ。手動 `git commit` を打たない。メッセージ変更は `--continue` 後に `git commit --amend`、または `.git/rebase-merge/message` を事前書き換え
- 復旧(手動 commit 後): `git reset --soft HEAD~1` → `git commit -F .git/rebase-merge/message` → `git rebase --skip`(skip は「現 HEAD を採用して次へ」として作用し rewritten-list を進める)
- 関連: `GIT_EDITOR=true` を `--continue` に渡すと commit-msg が空で abort、同じ誤エラーが出る。`GIT_EDITOR=:` も同様。メッセージ保持には `git commit -F .git/rebase-merge/message` を先に明示実行

## push 前の検査は結果を見てから push(検査と push を 1 コマンドに繋げない)

- 症状: 公開 repo への push 前に禁止語(実名・大会名等)を `git diff ... | grep` で検査したが、同じコマンドで `; git push` まで繋げたため、検査が hit しても push が進んで公開された
- 対処: 検査と push を別コマンドに分け、結果を読んでから push。1 コマンドにするなら hit で止まる形 (`! git diff ... | grep -qiE '<語>' && git push`)
- 禁止語は表記ゆれ(英字の綴り・区切り違い・カタカナ・略称)まで並べてから探す。実測: 英字の綴りだけで探しカタカナ表記を見落とし、履歴の書き換えが 2 回になった
- 漏れた後: 履歴から消すには force-push が要る。書き換えは、外部が commit ID で固定している commit(本番 deploy の ref 等)より後の範囲に限る(固定先が消えると deploy が壊れる)

## `git rm` の削除は即 stage され、別のファイルを指定した次の commit に混ざる

- 現象: `git rm <a>` → 後で `git add <b> <c>` だけを指定して commit しても、index にある `<a>` の削除が一緒に入る(commit を分けるつもりが崩れる)
- 対処: commit の直前に `git diff --cached --name-only` で中身を確かめる。commit を分けるなら、削除は作業ツリーで消し(`rm`)、入れたい commit で `git add <消したパス>` で stage する
- 未 push なら作り直し可: `git reset --soft <base>` → `git reset`(index を空に)→ commit ごとに add し直す

## 他人の未コミットの変更がある file で、自分の編集だけを stage する

- 状況: 同じ作業ツリーを別 session・別 tool も編集し、対象 file に他人の未コミットの変更がある。`git add <file>` は他人の変更も入れる。`git add -p` は対話で使えない
- 手段: `git stage-own-edit begin <file>` → 任意の tool で編集 → `git stage-own-edit stage <file>`(dotfiles `bin/git-stage-own-edit`、PATH 上)
  - begin 時点の写しと index を行で対応づけ、begin 以降に自分が変えた行だけを index に当てる。隣の行が他人の変更でも可。他人の行が上に増減していても自分の行の位置に当たる
  - 自分の編集が、写しと index で違う行(他人の変更)に触れると拒否(exit 1、index は不変)。新規 file は対象外(`git add`)
  - 改行は index の版に合わせる(autocrlf の CRLF 作業ツリー + LF の blob も可)
- 後: `git diff --cached` で確認 → commit。test: `bash bin/tests/git-stage-own-edit.test.sh`
