# Git on Windows の罠

## .git 再初期化で symlink・実行ビット消失

- 症状: 上流repo(テンプレ等)の `.git` を削除→`git init` し直すと、symlink(120000)は平文ファイル(中身=リンク先パス文字列)、実行ファイル(100755)は100644でcommitされる
- 検知: CIで `./script` が **exit 126**(permission denied)。symlinkだったファイルを cat するとパス文字列1行のみ
- 原因: Windows checkout は core.symlinks=false・core.filemode=false → 作業ツリーにmode情報なし、新indexは全て100644
- 復元(index直接操作。Windows FS上のファイルはそのままでよい):
  - 実行ビット: `git update-index --chmod=+x <file>`
  - symlink: `git update-index --cacheinfo "120000,<blob>,<path>"`(blob=リンク先パスの文字列。平文化したファイルの中身がリンク先パスそのものなら `git rev-parse :<path>` で流用可)
- 上流の正しいmode一覧: `gh api "repos/<owner>/<repo>/git/trees/HEAD?recursive=1" --jq '.tree[] | select(.mode!="100644" and .mode!="040000") | [.mode,.path] | @tsv'`
- 予防: 上流の履歴を切りたいだけなら `git checkout --orphan` や `git clone --depth 1` + 履歴squashの方がmode情報を保持できる
