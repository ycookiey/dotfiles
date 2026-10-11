# Node.js / tsx の罠

## `npx tsx -e` で top-level await が使えない

- 症状: `Top-level await is currently not supported with the "cjs" output format`。package.json に `"type": "module"` が無いプロジェクトでは `-e` も `.ts` ファイルも CJS で変換される
- 対処: 処理を `async function main() { ... } main()` で包む。`.mts` にすれば ESM として動く

## `zlib.zstdDecompressSync` が型エラー

- Node 22.15+ / 23.8+ には在るが、プロジェクトの `@types/node` が古いと型に無い。`(zlib as unknown as { zstdDecompressSync(b: Buffer): Buffer }).zstdDecompressSync` で使う

## tsconfig の target が ES2020 未満だと BigInt literal (`123n`) が型エラー

- Next.js の既定 tsconfig は `target: ES2017`。スクリプトでも `BigInt('0x...')` / `BigInt(1)` で書く (target を上げるとアプリ側に波及する)
