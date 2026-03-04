# yscoopy — Scoop バケット

リポジトリ: ycookiey/yscoopy

自作アプリの Scoop マニフェストを管理するバケットリポジトリ。

## リリース自動更新の仕組み


アプリリポジトリ ── GitHub webhook (release) ──> Cloudflare Worker ──> yscoopy repository_dispatch
                                                 (PAT 1箇所保管)        update.yml → scoop checkver -u


- 各アプリリポジトリにはPATを置かない
- Worker の URL を GitHub Webhooks に登録するだけ
- update.yml は日次 cron / 手動 / repository_dispatch で実行（checkver.ps1 で全マニフェスト一括チェック）

## アプリをリリースしたとき

webhook が設定済みなら何もしなくてよい。Worker 経由で自動更新される。

## 新しいアプリを yscoopy に追加する手順

`yscoopy-add.ps1` で一発実行:

```bash
pwsh -c '. $PROFILE; & "C:/Main/Project/dotfiles/bin/yscoopy-add.ps1" <AppName> <RepoName> "説明文"'
# 例: pwsh -c '. $PROFILE; & "C:/Main/Project/dotfiles/bin/yscoopy-add.ps1" yclocky yClocky "A minimal clock app for Windows"'
```

マニフェスト生成・yscoopyへのpush・webhook登録をすべて自動で行う。

### 前提

`~/.config/yscoopy.json` に Worker URL と WEBHOOK_SECRET を設定済みであること:
```json
{ "worker_url": "https://...", "webhook_secret": "..." }
```

## Worker 環境変数

wrangler secret put または Cloudflare ダッシュボードで設定。

| 変数名 | 用途 |
|--------|------|
| GITHUB_PAT | repository_dispatch 用。fine-grained token で contents:write on yscoopy |
| WEBHOOK_SECRET | webhook 署名検証用 |

### Secret のローテーション

ユーザーは WEBHOOK_SECRET の値を直接把握していない。変更が必要な場合:

1. openssl rand -hex 32 で新しい値を生成
2. cd yscoopy/worker && echo "<new>" | npx wrangler secret put WEBHOOK_SECRET
3. 全リポジトリの webhook を更新（gh api repos/ycookiey/<repo>/hooks/<id> -X PATCH -f "config[secret]=<new>"）

webhook 登録済みリポジトリの一覧とhook IDは gh api repos/ycookiey/<repo>/hooks -q '.[0].id' で確認できる。