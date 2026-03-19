日本語で回答.前置き,繰り返し不要.内容は省略しない.口調は簡潔に:を確認します→を確認,しました→した

# ルール

- シェルはGit Bash。Windowsパスは Unix形式で /c/path。Edit/Read は C:\path
- 未完成の実装にはTODOコメント必須
- IMPORTANT: コミットメッセージにAIツール名や Co-Authored-By を入れない
- ハマった問題: まず汎用的な学びを抽出し ~/.claude/docs/ に具体的ファイルで記録
- 不明点や困ったことは積極的にWebSearchで解決する
- 外部サービスのURLを案内時,クエリパラメータを活用しユーザ操作を減らす
- 対処法が複数あり得る問題は、最初に見つけた方法を即実装せず、トレードオフを比較提示してユーザが選んでから実装する

# rules/ と docs/ の使い分け

/.claude/rules/ は全プロジェクトで常時ロードされる（dotfilesからシンボリックリンク）。コンテキスト消費を抑えるため最小限に保つ。

- rules/ — 現在未使用。常時ロードが必要な指示が出てきたらユーザーに作成を提案
- docs/ — 必要時のみ読む（ツール別トラブルシュート,手順書等）

## 参照ドキュメント

該当する作業を始める前に必ず読む。 ドキュメントにある情報をユーザーに質問しない。

- PowerShellコーディング → ~/.claude/docs/powershell.md
- Cloudflare作業 → ~/.claude/docs/cloudflare.md
- モバイル開発 → ~/.claude/docs/mobile-dev.md
- Scoopバケット（yscoopy）リリース手順 → ~/.claude/docs/yscoopy.md
- agent-browser/browser-fetch → ~/.claude/docs/agent-browser.md
- シンボリックリンク構成確認・変更 → ~/.claude/docs/symlinks.md
- 環境を横断して再現したい変更（アプリ・設定等） → ~/.claude/docs/bootstrap.md
