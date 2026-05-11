---
name: ui-visual-check
description: UI要素の整列・余白・font-size等を視覚＋実測値で検証。Playwrightで高DPIスクショとDOM計測値(rect/computed style)を同時に取り、視覚と数値の両輪で突合する。生成ではなく検証用途。
---

`bash scripts/shoot.sh <url> <selector> [output_basename]` を実行する。

- `<url>`: 対象ページのURL（http/https/file://いずれも可）
- `<selector>`: CSS セレクタ（最初にマッチした要素を撮影・計測）
- `[output_basename]`: 省略時は timestamp。`.agent-output/ui-visual-check/<basename>.{png,json}` に出力

実行後の手順:
1. 出力の `PNG:` 行のパスを Read ツールで開いて視覚確認
2. 出力の `JSON:` 行のパスを Read で開いて rect / computed style を確認
3. 「中心は揃ってるが font-size が違う」のような視覚 vs 数値の不一致を突合する

before/after 比較したい場合は output_basename を変えて 2 回実行し、両方を Read で並べる。

失敗時はスクリプト自体を読んで修正し、スクリプト全体を再実行する。個別コマンドの手動実行で回避しない。
