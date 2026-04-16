---
name: restructure
description: ファイル構造変更(split/move)。引数なしで大ファイルを自動検出し候補提示。
allowed-tools: Bash, Read, Grep, Glob, Edit, Write, AskUserQuestion
---

# Restructure Skill

ファイル分割・移動を標準化。引数なし時は大ファイルを自動検出して候補提示。

## 操作種別

- **split**: 1ファイル → 複数（責務分離）
- **move**: ファイル/関数の別モジュール移動

## 自動検出（引数なし時）

以下を順に実行:

```bash
git ls-files | xargs wc -l 2>/dev/null | grep -v ' total$' | sort -rn | head -20
dotcli token-audit session --project "$PWD" --last 5 2>/dev/null
```

スコアリング（token-audit失敗/空なら行数のみ）:
- スコア = 行数 × (1 + 重複読み込み回数 × 0.5)
- 閾値: 300行以上を候補

結果をテーブル表示してユーザに対象選択を促す:

| # | ファイル | 行数 | 重複回数 | スコア |
|---|---------|------|---------|-------|
| 1 | src/foo.ts | 850 | 3 | 2125 |

## ワークフロー

1. **分析**: Read で対象ファイルの構造把握。exports・imports・論理ブロックを列挙
2. **計画**: 新ファイル構成を提案

   | 元ファイル | 新ファイル | 責務 |
   |-----------|-----------|------|
   | src/foo.ts | src/foo/core.ts | コアロジック |
   | src/foo.ts | src/foo/utils.ts | ユーティリティ |
   | src/foo.ts | src/foo/index.ts | re-export barrel |

3. **確認**: AskUserQuestion でユーザ承認
4. **実行**: ファイル作成/移動 → Grep で全import参照を特定 → import更新 → 必要ならre-export
5. **検証**: ビルド/テスト実行（`npm run build`, `cargo build`, etc. をプロジェクトに応じて）

## ルール

- 既存public APIを壊さない（re-exportでbarrelファイル維持）
- 1回の実行で1ファイル（or 密結合のファイル群）のみ
- 計画段階では実行しない。ユーザ承認後に実行
- 移動後は必ずimport参照を全更新（Grep → Edit）
