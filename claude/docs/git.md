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

## skip-worktree flag は worktree local state = process 間で漂う

`git update-index --skip-worktree <file>` は git config でなく worktree local state。process 間 (別 clone / fresh checkout / CI runner) で失われる。

- 「skip-worktree 下で運用」を前提にする generated file は、flag 再設定を script 化 (postinstall / dev 起動時) しないと process 間で状態が漂う
- 実例: `git ls-files -v <file>` = H (通常 tracked、skip-worktree なし) だが README/CLAUDE.md 記述は「skip-worktree 下」を前提 → `pnpm dev:web` / verify で M として現れ `git add -A` で意図しない commit の事故導線
- 設計判断: tracked + skip-worktree は「暗黙契約」= fragile、**gitignore + generated-only 経路が robust** (絶対に tracked にしない、build 時再生成のみ)

## Windows で git-credential-wincred が「CredWrite failed」

- 症状: HTTPS の clone 自体は成功するが「CredWrite failed」と出る。資格情報が保存されず、fetch / push のたびにパスワードを聞かれる
- 切り分け結果(2026-09): ダミーの資格情報でも `git credential-wincred store` が失敗 → 入力内容ではなく保存先の問題。GCM の `wincredman` ストアも「Unable to persist credentials」。一方 `cmdkey /generic:` では書き込める。VaultSvc 稼働中・`DisableDomainCreds=0` でも発生し、根本原因は未特定
- 本物の資格情報を汚さないテスト: ホストを `credtest.invalid` にして store → `cmdkey /list` で確認 → 同じ入力から password 行を除いて erase

```bash
printf 'protocol=https\nhost=credtest.invalid\nusername=test-user\npassword=dummy\n\n' | git credential-wincred store
cmdkey /list | grep credtest
printf 'protocol=https\nhost=credtest.invalid\nusername=test-user\n\n' | git credential-wincred erase
```

- 回避候補: GCM の DPAPI ストア(`credential.credentialStore=dpapi`)はダミーで保存・削除を確認済み。保存先は `~/.gcm/dpapi_store/`
- リポジトリ単位で GCM に切り替える場合、system gitconfig の wincred が先に走るので、local config で `credential.helper` を空値でリセットしてから `manager` を足す
- GCM はホストに通信してプロバイダを自動判定する。自前ホストの Gitea 等では `credential.provider=generic` を設定して判定を止める
- scoop の git は helper を system gitconfig(`<scoop>/apps/git/<ver>/etc/gitconfig`)で wincred に設定している

## 部分 stage の単独検証と worktree の後片付け(Windows + pnpm)

- 検証: 兄弟 dir に `git worktree add --detach <dir> HEAD` → `git diff --cached --binary > p.patch` を `git apply p.patch` → `pnpm install --offline --frozen-lockfile` → typecheck/lint/test。`link:../x` 依存は兄弟 dir なら同じ相対パスで解決
- 同一 file 内で commit を分ける: 片方の変更を除いた版を作り `git hash-object -w --path=<file> <版>` → `git update-index --cacheinfo 100644,<sha>,<file>`(非対話。`git add -p` 不可の環境向け)
- 確かめは CI の手順を全部(format:check を含む)。package.json の CI 用 script か workflow を見て並べる
- Windows の作業ツリー(autocrlf)では prettier --check が CRLF で全ファイル落ちる → コミットした中身で確かめる: `git show HEAD:<f> | prettier --check --stdin-filepath <f>`
- 後片付け: pnpm の node_modules(junction)で `git worktree remove --force` が "Directory not empty" で失敗(登録は外れる)。残骸は `cmd //c "rmdir /s /q C:\path"`(junction はリンクだけ消し、先をたどらない)。rm -rf の junction の扱いは環境依存のため使わない

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
- 編集前に begin し忘れないよう、hook(yhooky の組み込みの判定 `stage-own-edit-reminder`)が Edit/Write の直前に知らせる(追跡済み・HEAD との差分あり・begin 前のときだけ、session ごと file ごとに 1 回)

## ignore の対象の下にある追跡済み file は `git add <path>` が拒否される

- 現象: global ignore 等で `.claude/` が ignore 対象の repo で、追跡済みの `.claude/skills/x/SKILL.md` を `git add <path>` → 「The following paths are ignored」で何も stage されない(追跡済みでも)
- 対処: `git add -u -- <path>`(追跡済み file の変更だけを stage)。新規 file を足すときだけ `git add -f`

## autocrlf の作業ツリーで stash / checkout した後、スクリプトの置換が一致しない

- 現象: `core.autocrlf=true` (Windows) で `git stash` → `stash pop`、`git checkout -- <file>` をすると、git が書き戻したファイルは CRLF になる。LF に直して編集していたファイルも CRLF に戻る
- 影響: python や sed で `"...\n..."` を含む複数行を完全一致で置換するパッチが「見つからない」で失敗する。文字化けや日本語の問題に見えて原因を取り違えやすい
- 対処: 置換前に `file <path>` で改行を確かめる。複数行の置換は Edit ツールで行う (改行の違いを吸収する)。スクリプトで置換するなら読み込み時に `\r\n` → `\n` に正規化する
- stash を使った前後比較 (変更前の版でスクリプトを走らせる等) は、この書き戻しが起きる操作だと意識しておく
