# pnpm

## Windows で `link:` 依存を追加すると区切りがバックスラッシュになる

- 現象: `pnpm add "<pkg>@link:../path"` → package.json と pnpm-lock.yaml の specifier が `link:..\\path`
- 影響: Linux では区切りと解釈されない。片方だけ直すと `--frozen-lockfile` が manifest と lockfile の不一致で失敗
- 対策: 追加後に両ファイルの specifier を `/` に直す → `pnpm install --frozen-lockfile` で整合確認

## better-sqlite3 13 の install が C++ のビルドで止まる(Windows)

- 現象: 新しい git worktree などで `pnpm install` → better-sqlite3 が node-gyp → MSBuild の ClangCL ツールセットで失敗し、install 全体が ELIFECYCLE で止まる(Node 24、VS 2019 Build Tools に ClangCL なし)
- 原因: package に `binding.gyp` があると、install の script がなくても pnpm が暗黙に `node-gyp rebuild` を走らせる
- 対策: 実行時は同梱の `prebuilds/win32-x64.node`(`lib/binding.js` が読む)を使うのでビルド不要。`pnpm install --ignore-scripts` で入れ、ビルドと test が通るか確認。他に install の script が要る package だけ `pnpm rebuild <name>`
