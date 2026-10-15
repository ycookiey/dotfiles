# Node.js / tsx の罠

## `npx tsx -e` で top-level await が使えない

- 症状: `Top-level await is currently not supported with the "cjs" output format`。package.json に `"type": "module"` が無いプロジェクトでは `-e` も `.ts` ファイルも CJS で変換される
- 対処: 処理を `async function main() { ... } main()` で包む。`.mts` にすれば ESM として動く

## `zlib.zstdDecompressSync` が型エラー

- Node 22.15+ / 23.8+ には在るが、プロジェクトの `@types/node` が古いと型に無い。`(zlib as unknown as { zstdDecompressSync(b: Buffer): Buffer }).zstdDecompressSync` で使う

## tsconfig の target が ES2020 未満だと BigInt literal (`123n`) が型エラー

- Next.js の既定 tsconfig は `target: ES2017`。スクリプトでも `BigInt('0x...')` / `BigInt(1)` で書く (target を上げるとアプリ側に波及する)

## tsx で速さを測る・プロファイルすると実際より重く出る

- tsx は esbuild を `keepNames` で使い、関数 (閉包を含む) を作るたびに `__name(fn, "name")` を挟む。閉包の多い処理では `--cpu-prof` の上位に `__name` が出る (実例: tick sim で約 7%)
- 対処: 計測・プロファイルは `esbuild <entry.ts> --bundle --platform=node --format=esm --outfile=<out>.mjs` でまとめて `node <out>.mjs`。tsconfig の paths (`@/...`) は esbuild がそのまま解決する。出力は gitignore 済みの `node_modules/.cache/` に置く
- 経過時間はほかの処理との CPU の取り合いで ±15% ほど揺れる。`process.cpuUsage()` の CPU 時間を併記する。計算量の比較は処理の回数 (例: 回した tick 数) を数えて負荷と切り離す
- Windows の CPU 時間は約 15.6 ms 刻み。実例: 数十 ms の処理を 1 回ずつ測ると 16・31・47・77 ms と刻みの倍数になり、倍率を 1.86 と誤った (30 回まとめると 2.45)。1 回が数百 ms に満たない処理は繰り返しをまとめて 1 回で測り、回数で割る。最小値・中央値で比べるのは 1 回が十分長いときだけ

## Rust→wasm の重い所を探す・速さを比べる

- `node --cpu-prof` は wasm の呼び出しの列をたどれないことが多い (sample の多くが root 直下)。inclusive は当てにならず self だけ使う
- 関数名には name section: `wasm-pack build --profiling` + Cargo.toml `[package.metadata.wasm-pack.profile.profiling] wasm-opt = ["-O", "-g"]`。無いと `wasm-function[98]`
- LTO で inline された関数は呼んだ側の self に入る。内訳はネイティブ build を samply で
- ネイティブの profile は確保・解放を wasm より重く見せる (Windows HeapAlloc、dlmalloc にしても lock)。実例: 確保を減らす変更がネイティブ 35→20 秒、wasm 29→27 秒。速さは wasm で判定
- 版の比較は交互に複数回、最小値・中央値で (1 回ずつは ±10% 揺れて逆の結論になった)

## CI の npm の版が lock を作った npm とずれると `npm ci` が落ちる

- 現象: CI で npm を浮動 (`npm i -g npm@^11.x`、setup-node の `node-version: 24` で最新) にすると、手元で lock を作った npm と版がずれる。peer 依存の扱いの差で `npm ci` が `EUSAGE` / `Missing: <pkg>@<ver> from lock file` (実例: 手元 11.6.1 の lock を CI の 11.21.0 が拒否。不足は `@napi-rs/wasm-runtime` の peer `@emnapi/*`)
- 対処: CI の Node を手元と同じ patch 版に固定し、同梱の npm を使う (`node-version: 24.11.0` → npm 11.6.1)。npm を上げるなら手元も同じ版で lock を作り直す
- push 前の再現: WSL で clone して `npx -y npm@<版> ci`。手元の Windows の `npm ci` では版の差を再現できない

## npm の trusted publishing (OTP なしで CI から公開)

- 2FA を素通りする granular token は、アカウントの変更が 2026-08 から対話の 2FA 必須、直接 publish が 2027-01 に廃止予定。CI からの公開は trusted publishing (OIDC) か staged publishing
- 登録: npmjs.com の `/package/<name>/access` (Settings) → Trusted Publisher → GitHub Actions。org / repo / workflow のファイル名 (`.github/workflows/` 内の名前だけ)。保存後は変更できず削除して作り直す。開くときに 2FA (security key 等) を求められる
- 「Allow npm publish」をオフにすると直接 publish できず、毎回 npmjs.com で承認する staged publishing になる (`npm stage publish` は常に許可)
- 登録後、期限 (2 日) 内の初回の publish で検証される。検証前は Status が Pending validation
- workflow: `permissions: id-token: write`、setup-node に `registry-url: https://registry.npmjs.org`、npm 11.5.1 以上 (Node 24.11.0 同梱の 11.6.1 で可)。公開 repo・公開 package なら provenance が自動で付く。`package.json` の `repository.url` は repo と一致させる
- 手元の `npm publish` は 2FA の OTP (`EOTP`) を求める。trusted publishing にしたら手元で publish しない
- 公開直後: registry の `dist-tags.latest` は 1 分弱で更新、unpkg は先に引いた 404 をしばらくキャッシュする。jsDelivr か `npm pack <name>@<ver>` の tarball から取る
