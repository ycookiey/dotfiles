# ローカル port 予約表

新規プロジェクトで固定 port を決める際、ここに記録済みの port を避けること。
決めたら**必ずこの表に追記**する（追記しないと次プロジェクトで衝突する）。

## 予約済み

| port  | プロジェクト / 用途                          | 備考 |
|-------|--------------------------------------------|------|
| 3939  | AnalyzerCH / packages/web (Next.js dev)    | `scripts/dev-web.sh` |

## 避けるべき（フレームワークデフォルト）

| port         | 衝突対象                              |
|--------------|-------------------------------------|
| 3000         | Next.js / CRA / Express / Rails API |
| 4200         | Angular                             |
| 4321         | Astro                               |
| 5173         | Vite                                |
| 6006         | Storybook                           |
| 8080         | Tomcat / 各種 proxy                 |
| 8081         | Metro (React Native)                |
| 8787         | Cloudflare Wrangler                 |
| 9000         | PHP-FPM / SonarQube                 |
| 19000-19002  | Expo                                |
| 49152-65535  | OS ephemeral range                  |
