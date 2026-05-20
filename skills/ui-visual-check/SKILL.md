---
name: ui-visual-check
description: UI要素の整列・余白・font-size等を視覚＋実測値で検証。Playwrightで高DPIスクショとDOM計測値(rect/computed style)を同時に取り、視覚と数値の両輪で突合する。生成ではなく検証用途。
---

`bash scripts/shoot.sh <url> <selector> [output_basename]` を実行する。

- `<url>`: 対象ページのURL（http/https/file://いずれも可）
- `<selector>`: CSS セレクタ（最初にマッチした要素を撮影・計測）
- `[output_basename]`: 省略時は timestamp。`.agent-output/ui-visual-check/<basename>.{png,json}` に出力

出力は3種:
- `PNG:` 素のスクショ（実際の見た目）
- `GUIDE:` 整列ガイド注入版（`<basename>-guides.png`）。対象要素の端=赤線・中心=橙線をページ全体に延ばし、兄弟要素を青枠で囲む。基準線上に兄弟のエッジが乗っているか＝整列のズレを視覚で炙り出す。撮影範囲は兄弟が入る親要素（ビューポート交差）
- `JSON:` rect / computed style に加え `siblings`（親の直接の子の left/right/centerX/top/bottom/centerY/width/height、対象は `self:true`）と `parentRect` を含む

実行後の手順:
1. `PNG:` 行を Read で開いて実際の見た目を確認
2. `GUIDE:` 行を Read で開いて整列のズレを視覚確認（線と兄弟枠のエッジ一致を見る）
3. `JSON:` 行を Read で開き、`siblings` の left/centerX 等が揃っているか数値で突合
4. 「中心は揃ってるが font-size が違う」「左端が1px ズレてる」のような視覚 vs 数値の不一致を突合する

要素が大きい場合（実ピクセル = CSS寸法 × 3 が上限超過）、`PNG:`/`GUIDE:` 行が複数出力され `<basename>[-guides]-r<行>c<列>.png` に縦横分割される（`TILES:` / `GUIDE_TILES: 行x列`）。各系統を左上→右下の順に Read で開く。`warn:` が出たら範囲が広すぎるので、より具体的なセレクタの再指定を検討する。
閾値は `UI_CHECK_MAX_EDGE`（既定1500px）、DPIは `UI_CHECK_SCALE`（既定3）で調整可。

before/after 比較したい場合は output_basename を変えて 2 回実行し、両方を Read で並べる。

失敗時はスクリプト自体を読んで修正し、スクリプト全体を再実行する。個別コマンドの手動実行で回避しない。
