---
name: restructure
description: ファイル構造変更(split/move/consolidate)。引数なしで大ファイル・重複コードを自動検出し候補提示。
allowed-tools: Bash, Read, Grep, Glob, Edit, Write, AskUserQuestion
---

# Restructure Skill

ファイル分割・移動・重複統合を標準化。引数なし時は大ファイルと重複コードを自動検出して候補提示。

## 操作種別

- **split**: 1ファイル → 複数（責務分離）
- **move**: ファイル/関数の別モジュール移動
- **consolidate**: 複数箇所の重複コードを共通モジュールに抽出

## 自動検出（引数なし時）

以下を順に実行:

```bash
# (1) 大ファイル候補
git ls-files | xargs wc -l 2>/dev/null | grep -v ' total$' | sort -rn | head -20
dotcli token-audit session --project "$PWD" --last 5 2>/dev/null

# (2) コード構造の重複（type-2 / type-3 clone候補）
dotcli dup-scan --top 10 | dotcli dup-scan-format
# 「ほぼ同じだが数 token だけ違う」(operator 違い等) も拾う場合 (type-3)
dotcli dup-scan --gap 3 --top 10 | dotcli dup-scan-format

# (3) 文字列リテラル重複（Tailwind/SQL/prompt等）
dotcli string-dup --top 10 | dotcli string-dup-format
# 類似 (Tailwindの細部差等) も拾う場合
dotcli string-dup --similarity jaccard --threshold 0.7 --top 10 | dotcli string-dup-format
```

対象は `git ls-files` に含まれるリポジトリ内ファイルのみ。token-audit / dup-scan / string-dup がリポジトリ外パスを返したら除外。

### split/move スコアリング（token-audit失敗/空なら行数のみ）

- スコア = 行数 × (1 + 重複読み込み回数 × 0.5)
- 閾値: 300行以上（`>=300`）を候補、スコア降順でソート

結果をテーブル表示してユーザに対象選択を促す:

| # | ファイル | 行数 | 重複回数 | スコア |
|---|---------|------|---------|-------|
| 1 | src/foo.ts | 850 | 3 | 2125 |

### consolidate 候補（dup-scan / string-dup）

**コード構造重複 (`dup-scan`)**:
- `match_tokens >= 50` かつ `occurrence_count >= 2`
- 異なる file をまたぐ cluster を優先（同一 file 内重複は split で扱える）
- `match_type`: `exact` (完全一致) / `type3` (gap 許容、operator や定数の差を吸収) — type3 は引数化や trait 化での吸収を検討
- `fallback_lexer_used` に該当する file の cluster は精度低下を注釈

| # | type | match+gap | lines | 出現箇所 |
|---|---|---|---|---|
| 1 | exact | 193+0 | 23 | src/cb.rs:14, src/cg.rs:42 |
| 2 | type3 | 131+1 | 10 | claude/statusline/src/main.rs:119, cli/.../resume.rs:603 |

**文字列重複 (`string-dup`)**:
- exact_clusters: 完全一致した文字列 (定数化候補)
- similar_clusters (jaccard opt-in): 似た class soup / SQL / prompt 群 (テンプレート化候補)

| # | type | chars | occ | 内容 |
|---|---|---|---|---|
| 1 | exact | 64 | 4 | "flex items-center gap-2 px-4 py-2 ..." |
| 2 | similar | ~50 | 6 | 4 variants sharing `flex items-center` base |

## ワークフロー

### split / move

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

### consolidate

1. **分析**:
   - dup-scan cluster: preview と各 occurrence を Read で確認。差分 (識別子・リテラル) を抽出して「共通化可能な範囲」を見極める
   - string-dup cluster: 各 occurrence を Read で文脈確認 (どの attribute / 関数引数で使われるか)
2. **計画**: 抽出方式を提示

   コード重複の場合:

   | 抽出元 | 抽出関数 | 配置先 | 引数 |
   |---|---|---|---|
   | cb.rs:14-36 | build_claude_args | common/args.rs | mode: Mode |
   | cg.rs:42-64 | (同上) | (同上) | (同上) |

   文字列重複の場合:

   | 抽出元 | 抽出形式 | 配置先 | 名前 |
   |---|---|---|---|
   | foo.tsx:23, bar.tsx:45 | const string | styles.ts | BTN_PRIMARY_CLASSES |
   | a.rs:10, b.rs:30 | format! template | constants.rs | SYNC_ERROR_FMT |

3. **確認**: AskUserQuestion で承認
4. **実行**:
   - コード: 共通モジュール作成 → 各 occurrence を関数呼び出しに置換 → 差分パラメータを引数化
   - 文字列: 定数 or テンプレート定義 → 各 occurrence を import + 参照に置換
5. **検証**: ビルド/テスト + 再 `dup-scan` / `string-dup` で cluster が消えたことを確認

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
- 1回の実行で1ファイル（or 密結合のファイル群、または1 cluster）のみ。**密結合 = 相互 import、または同一内部モジュール階層で分離不能**
- 計画段階では実行しない。ユーザ承認後に実行
- 移動後は必ずimport参照を全更新（Grep → Edit）
- consolidate では「差分が大きすぎて引数で吸収できない」cluster は対象外（無理に統合せず却下）
