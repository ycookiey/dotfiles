# 外部サービスURLプリフィルテンプレ

URL案内時にユーザ操作を減らすためのクエリパラメータ集。対応可否不明のサービスは案内前にWebSearchで確認し、判明したらここに追記。

## GitHub fine-grained PAT 作成

- `https://github.com/settings/personal-access-tokens/new?name=<40字以内URLenc>&description=<1024字以内URLenc>&target_name=<owner>&expires_in=<1-366|none>&<permission>=<read|write|admin>`
- permission は `&` 連結(例 `contents=write&pull_requests=write`)。metadata:read 自動付与
- リポジトリ選択(Only select repositories)はプリフィル不可 → 手動箇所と明記して案内
- 例(repository_dispatch 用): `.../new?name=repo-dispatch&target_name=<owner>&expires_in=366&contents=write`
- 出典: docs.github.com "Managing your personal access tokens" / GitHub changelog 2025-08-26

## Cloudflare Pages project 作成

- `https://dash.cloudflare.com/<account id>/workers-and-pages/create/pages`: Pages の「Get started」(Git から取り込むか、ファイルを直接上げるかを選ぶ画面)に直接入る(2026-10-15 確認)
- Git 連携の repo 選択以降はプリフィル不可

## GitHub classic PAT 作成

- `https://github.com/settings/tokens/new?scopes=repo,workflow&description=<説明>`
- fine-grained で足りる場合はそちらを優先(最小権限)
