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

対象は `git ls-files` に含まれるリポジトリ内ファイルのみ。token-audit がリポジトリ外パスを返したら除外。

スコアリング（token-audit失敗/空なら行数のみ）:
- スコア = 行数 × (1 + 重複読み込み回数 × 0.5)
- 閾値: 300行以上（`>=300`）を候補、スコア降順でソート

結果をテーブル表示してユーザに対象選択を促す:

| # | ファイル | 行数 | 重複回数 | スコア |
|---|---------|------|---------|-------|
| 1 | src/foo.ts | 850 | 3 | 2125 |

## ワークフロー

1. **分析**: Read で対象ファイルの構造把握。exports・imports・論理ブロックを列挙
2. **計画**: 新ファイル構成をチャット本文のテーブルで提示

   | 元ファイル | 新ファイル | 責務 |
   |-----------|-----------|------|
   | src/foo.ts | src/foo/core.ts | コアロジック |
   | src/foo.ts | src/foo/utils.ts | ユーティリティ |
   | src/foo.ts | src/foo/index.ts | re-export barrel |

3. **確認**: AskUserQuestion でユーザ承認（使えない環境では本文で選択肢を列挙して回答を促す）
4. **実行**: ファイル作成/移動 → Grep で全import参照を特定 → import更新 → 必要ならre-export
5. **検証**: ビルド/テスト実行（`npm run build`, `cargo build --release`, etc. をプロジェクトに応じて）

## 粒度・配置・命名

- **粒度**: 論理ブロック境界で分割。目安は分割後 1ファイル 200-400 行
- **配置**: 元ファイル名の同階層サブディレクトリを優先（`foo.ts` → `foo/*.ts`）。既存慣習があればそれに従う
- **命名**: 責務を示す短い名詞（`core`, `utils`, `scan`, `format` 等）。言語慣習に従う
- **対象除外**: 自動生成物（`*.lock`, `package-lock.json`, `*.generated.*`）は自動検出から除外

## 言語別 public API 維持

| 言語 | ファイル→ディレクトリ化 | 再エクスポート |
|---|---|---|
| JS/TS | `foo.ts` → `foo/index.ts` | `export * from './core'` |
| Rust | `foo.rs` → `foo/mod.rs` + `mod xxx;` | `pub use xxx::Name;` |
| Python | `foo.py` → `foo/__init__.py` | `from .core import *` |
| PowerShell | dot-source チェーン（`. "$PSScriptRoot\sub.ps1"`） | 親スコープに関数/変数継承、export 不要 |

## ルール

- 既存public APIを壊さない（各言語の再エクスポート方式で維持）
- 1回の実行で1ファイル（or 密結合のファイル群）のみ。**密結合 = 相互 import、または同一内部モジュール階層で分離不能**
- 計画段階では実行しない。ユーザ承認後に実行
- 移動後は必ずimport参照を全更新（Grep → Edit）
