# Cloudflare アカウント

| アカウント | Account ID | 用途 |
|-----------|------------|------|
| (チーム共有) | `wrangler whoami` で確認 | UC プロジェクト |
| ycookiey | 0ae949d229d9cc0aacd580a49bcc15ca | 個人プロジェクト |

## ドメイン
- analyzers.app — 分析ツール群（複数プロジェクト共用）
- omega-analyzer.com
- ycookiey.com

## 使い方

wrangler で複数アカウントがある場合:
bash
CLOUDFLARE_ACCOUNT_ID=<id> npx wrangler ...


API token は 2 か所: User API Tokens(`/profile/api-tokens`)と Account API tokens(`/<account id>/api-tokens`)。CI 用は Account 側にあることがある(Last used で特定)。cf の OAuth ではどちらの一覧も 403

## Gotchas

### React 19 SSR: MessageChannel is not defined

**症状**: Pages デプロイ時に Uncaught ReferenceError: MessageChannel is not defined

**原因**: CF アダプターが react-dom/server → react-dom/server.browser にエイリアスするが、React 19 の同モジュールが MessageChannel を使用。Workers ランタイムにはこの API がない。

**解決策**: astro.config.mjs の Vite alias で本番時のみ .edge 版を使用:
js
'react-dom/server': 'react-dom/server.edge'


**参考**: [astro#12824](https://github.com/withastro/astro/issues/12824)

### Pages GitHub連携の注意点

- Direct Uploads で作成したプロジェクトは後から GitHub 連携に変更不可。最初からダッシュボードで GitHub 連携で作成する必要がある。
- wrangler.toml を Pages で認識させるには pages_build_output_dir が必須。

### Vite プラグイン(@cloudflare/vite-plugin): dev の transformIndexHtml に絶対パス

- 症状: 自作 Vite プラグインの `transformIndexHtml` でページ判定 → dev のみ `Error: Unexpected error. Failed to load "/index.html"`(真因は cause 側でログに出ない)
- 原因: dev は HTML 返却時 `viteDevServer.transformIndexHtml(<HTML の絶対パス>, html)` を呼ぶ → `ctx.filename` が root 配下前提と不一致。`path.relative(root, ctx.filename)` で判定不可
- 対処: `ctx.filename` を `/` 区切りに正規化 → ページ HTML の相対パスと末尾一致で判定。build 時は影響なし
- 確認版: @cloudflare/vite-plugin 1.60.2 / Vite 8.3

### Vite プラグイン: 環境の選択と build 後の設定

- 環境(`env.<name>`)は build 時に `CLOUDFLARE_ENV=<name>` で選ぶ。build 後の `wrangler deploy --env` は無効(公式)。出力 `dist/<top-level の name>/wrangler.json` に選んだ環境が平坦化される(dir 名は top-level の name のまま)
- build 後は `.wrangler/deploy/config.json` が出力を指し、`wrangler deploy` はそれを使う。`wrangler d1 ...` は元の wrangler.jsonc + `--env` に従う(実測)
- 確認: 出力の wrangler.json の `name` / `routes` / `d1_databases` / `targetEnvironment` を見る。`wrangler deploy --dry-run` は未ログインでも通る
- 確認版: wrangler 4.142 / @cloudflare/vite-plugin 1.60

### D1: SQLite より SQL の上限が低い

- GLOB / LIKE: 型が長いと `LIKE or GLOB pattern too complex` で失敗(`[0-9]` を 20 個並べた日時の型で発生)。複合 SELECT: 項が多いと `too many terms in compound SELECT`(`UNION ALL` 10 項で発生)
- 罠: CHECK 内の GLOB は CREATE TABLE では通り、行を入れた時点で初めて失敗 → テーブルの migration だけ流しても気づけない。行を入れる migration かテストまで流して確かめる
- 日時の形式の CHECK は GLOB を使わず整形し直して比較: `coalesce(strftime('%Y-%m-%dT%H:%M:%SZ', col) = col, 0) OR coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', col) = col, 0)`。`T` や `Z` のない値、時差付き、2 月 30 日も断る(`coalesce` がないと NULL で CHECK が素通り)
- 確認環境: wrangler 4 のローカル D1(workerd)

## cf CLI(2026-09-28 open beta、npm `cf`)

- 確認版 1.0.0-beta.5。`npm i -g cf`(Node 22+)。wrangler は beta 終了後 18 か月保守
- command 探索: `cf cli search "<やりたいこと>"` → `<command> --help`(help の指示。nested `--help` を辿らない)。検索語に名前・ID を入れない
- 認証: `cf auth login`(device flow。手元 PC は可、datacenter IP は弾かれる: cloudflare/cf#23)。CI は `CLOUDFLARE_API_TOKEN`。OAuth の scope では API token 一覧(`cf user tokens list` / `cf accounts tokens list`)が 403
- zone 指定は `-z <ドメイン名>` で可。作成系は `--dry-run` で body を確認してから実行。個別 flag がない操作は `--body '<JSON>'`(例: DNS record 作成)
- D1: `cf d1 create --name <名前> --primary-location-hint apac`。location は作成時のみ指定可。wrangler deploy の自動作成は依頼元の近く(CI なら GitHub runner)→ 日本向けは手で作り `database_id` を config に書く
- `cf d1 migrations apply <DB ID>`: ID のみ(名前・binding 不可)。記録 table は wrangler 互換
- 罠: `cf migrate` は wrangler の env を `switch (ctx.mode)` に変換。Vite build の既定 mode は `production` → env 名 `production` は指定なしで選ばれる → 本番の env は `prod` 等にする(migrate も警告)。migrate の出力は TODO(`migrations_dir` 等)を埋めるまで throw する