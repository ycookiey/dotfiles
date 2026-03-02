# Cloudflare アカウント

| アカウント | Account ID | 用途 |
|-----------|------------|------|
| (チーム共有) | `wrangler whoami` で確認 | UC プロジェクト |
| ycookiey | 0ae949d229d9cc0aacd580a49bcc15ca | 個人プロジェクト |

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