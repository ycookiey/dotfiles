---
name: token-audit
description: Token consumption audit. Analyze JSONL logs and config files for optimization.
allowed-tools: Bash, Read, Glob, Grep
---

# Token Audit

`token-audit` CLIでトークン消費を分析し、改善点を提示する。

## 出力の取り扱い（必読）

- **生JSONを `head` / `tail` で切るな**。`by_tool` 配列が長くて後続section（`top_reads`, `duplicate_reads` 等）が見えなくなる事故が起きる
- 全体俯瞰は `token-audit-format` 経由（表形式・コンパクト）
- 特定sectionだけ欲しい時は section別subcommand を使う（下記）
- どうしても生JSONから抜き出すなら `jq '.duplicate_reads'` のように key 指定

## 実行手順

1. `dotcli token-audit static | dotcli token-audit-format` → 設定ファイル静的分析（表形式）
2. `dotcli token-audit session --last 3 | dotcli token-audit-format` → セッション分析（全section表形式）
3. または `dotcli token-audit all --last 3` で static + session 一括（formatter内蔵）
4. 深掘りは section別subcommand:
   - `dotcli token-audit session by-tool --last N`
   - `dotcli token-audit session top-reads --last N`
   - `dotcli token-audit session duplicates --last N`
   - `dotcli token-audit session large-ops --last N`
   - `dotcli token-audit session large-responses --last N`
   - `dotcli token-audit session toolsearch --last N`
   - `dotcli token-audit session startup --last N`
   - `dotcli token-audit session growth --session ID`（単一session専用）
   - 各々 `| dotcli token-audit-format` でも整形可

オプション: `--all` / `--last N` / `--session ID` / `--project PATH`

5. 結果を解釈し、以下の観点で改善点を提示:

### 静的分析の観点

- CLAUDE.md推定トークン > 1500 → 圧縮推奨（⚠）
- skill description合計 > 800 → 個別圧縮推奨（⚠）
- plugin数 > 3 → 不要plugin無効化を検討（⚠）
- .claudeignore未設定 → 作成推奨（⚠）

### セッション分析の観点

- ツール行で cache_create > 5000tok → 部分読み・出力制限を推奨（⚠）
- 同一ファイルの重複Read（`duplicate_reads`）→ 初回で必要部分をメモし再読み不要に
- 起動コスト（`startup_costs.first_total_ctx`）の傾向を報告
- compaction頻度と圧縮率を報告

## 出力形式

JSONを `dotcli token-audit-format` でバー付き表に整形。数値はカンマ区切り（例: 31,992 tok）。

`by_tool` は top 15 件まで表示し、残りは `(others N tools)` に集約される（生JSONも同様）。
