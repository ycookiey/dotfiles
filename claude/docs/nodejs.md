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
