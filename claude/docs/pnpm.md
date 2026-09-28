# pnpm

## Windows で `link:` 依存を追加すると区切りがバックスラッシュになる

- 現象: `pnpm add "<pkg>@link:../path"` → package.json と pnpm-lock.yaml の specifier が `link:..\\path`
- 影響: Linux では区切りと解釈されない。片方だけ直すと `--frozen-lockfile` が manifest と lockfile の不一致で失敗
- 対策: 追加後に両ファイルの specifier を `/` に直す → `pnpm install --frozen-lockfile` で整合確認
