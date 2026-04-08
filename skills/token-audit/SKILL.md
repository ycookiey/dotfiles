---
name: token-audit
description: Token consumption audit. Analyze JSONL logs and config files for optimization.
allowed-tools: Bash, Read, Glob, Grep
---

# Token Audit

`token-audit` CLIでトークン消費を分析し、改善点を提示する。

## 実行手順

1. `dotcli token-audit static` を実行 → 設定ファイルの静的分析結果（JSON）
2. `dotcli token-audit session` を実行 → JSONLログのセッション分析結果（JSON）
3. 人間向け表形式: `dotcli token-audit static | dotcli token-audit-format` または `dotcli token-audit session --last 3 | dotcli token-audit-format`
4. 結果を解釈し、以下の観点で改善点を提示:

### 静的分析の観点

- CLAUDE.md推定トークン > 1500 → 圧縮推奨（⚠）
- skill description合計 > 800 → 個別圧縮推奨（⚠）
- plugin数 > 3 → 不要plugin無効化を検討（⚠）
- .claudeignore未設定 → 作成推奨（⚠）

### セッション分析の観点

- ツール行で cache_create > 5000tok → 部分読み・出力制限を推奨（⚠）
- 同一ファイルの重複Read → 初回で必要部分をメモし再読み不要に
- 起動コスト（first_total_ctx）の傾向を報告
- compaction頻度と圧縮率を報告

## 出力形式

JSONを `dotcli token-audit-format` でバー付き表に整形。数値はカンマ区切り（例: 31,992 tok）。
