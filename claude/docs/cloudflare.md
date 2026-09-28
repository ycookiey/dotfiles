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

### D1: SQLite より SQL の上限が低い

- GLOB / LIKE: 型が長いと `LIKE or GLOB pattern too complex` で失敗(`[0-9]` を 20 個並べた日時の型で発生)。複合 SELECT: 項が多いと `too many terms in compound SELECT`(`UNION ALL` 10 項で発生)
- 罠: CHECK 内の GLOB は CREATE TABLE では通り、行を入れた時点で初めて失敗 → テーブルの migration だけ流しても気づけない。行を入れる migration かテストまで流して確かめる
- 日時の形式の CHECK は GLOB を使わず整形し直して比較: `coalesce(strftime('%Y-%m-%dT%H:%M:%SZ', col) = col, 0) OR coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', col) = col, 0)`。`T` や `Z` のない値、時差付き、2 月 30 日も断る(`coalesce` がないと NULL で CHECK が素通り)
- 確認環境: wrangler 4 のローカル D1(workerd)