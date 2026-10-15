---
name: ui-before-after
description: 画面の変更前後をスクリーンショットで見比べるページ(1 つの HTML)を作る。変わった所だけを切り出し、同じ枠の中で前後を切り替え、変わった範囲を赤い枠で示す。試しの実装の採否を見た目で決めてもらうとき、UI の変更を Owner に見せるときに使う。撮影は別(Playwright 等)で済ませておく。
---

`node ~/.claude/skills/ui-before-after/scripts/build.mjs --images <dir> --config <config.mjs> --out <page.html>` を実行する。依存は node と ImageMagick 7(`magick`。別の場所なら `UI_BEFORE_AFTER_MAGICK`)。

## 入力

- 画像: `<dir>/<key>-<before|after>-<幅>.png`。`<幅>` は `sp`・`pc` など任意の名前(file 名から拾う)。片方しかない場面は「変更後にしか起きない」などと出す
- 設定(`.mjs` の default export か `.json`):
  - `title`(タブの名前。2〜4 語の名前)、`heading`(h1)、`intro`(段落の配列。`` `code` `` は `<code>` になる)
  - `widths`: `{ <幅>: { label, maxWidth, frameMaxWidth } }`。並びが切り替えのボタンの順。`maxWidth` は埋め込む画像の幅の上限(PC の 1280px を 960px に縮めるなど)、`frameMaxWidth` は枠の幅の上限(スマホ幅を広げすぎない)
  - `defaultWidth`、`storageKey`(選んだ幅を覚えておく localStorage の key)、`summary: false`(冒頭の一覧を出さない)、`text`(画面の文言の差し替え。既定は page.mjs の `DEFAULT_TEXT`)
  - `groups`: 項目の配列。`{ id, label, title, where, kind, now, after, facts: [], rec, shots: [{ key, caption, note }] }`。`shots` は `[key, caption]` の組でもよい。`label` の代わりに `items` も読む。説明の欄はすべて省略できる
- 引数: `--margin`(切り出しの上下の余白 px、既定 32)、`--threshold`(画素の違いとみなす 3 色の差の和、既定 48)、`--gap`(これより近い違いを 1 つの枠にまとめる px、既定 48)

## 出力

- 1 つの HTML(画像は WebP の data URI)。16MB を超えると警告する(Artifact の上限)
- 標準出力に、場面ごとの変わった行(`before 高さ@位置 after 高さ@位置`)か `no difference`
- ページ: 項目ごとの説明 → 場面ごとに「変わった所」だけを切り出した枠。ボタン・← →・1 2・画像を押すで前後を切り替え、赤い枠が変わった範囲。画面全体は「画面全体を開く」

## 差分の見つけ方

- 行ごとの hash を並べて、前後の行を対応づける(patience diff。一意の行がない範囲は LCS)。上の要素が伸びて下が押し下げられただけの行は、変わった所に数えない
- 対応のない行のかたまりが「変わった所」。前後で同じ高さなら画素で比べて枠を絞り、違う高さなら行ごと枠にする
- 前後の切り出しは同じ高さに白で埋めるので、切り替えても枠の高さは変わらない

## 撮るときの注意

- 撮るたびに変わる値(乱数の名前、今の時刻、連番)は固定する。そうしないと、その違いも赤い枠になる
- 前後は同じデータ(同じ local の DB 等)で撮る。片方だけ件数が増えると、一覧の行の違いが出る
- PC 幅とスマホ幅は、同じ場面を `-pc`・`-sp` の名前で撮る

## 公開

Artifact で公開するときは、出来た HTML の file をそのまま publish する。同じ file の path で publish し直すと URL が保たれる。

## test

`node ~/.claude/skills/ui-before-after/scripts/diff.test.mjs`(小さな画像を作って、差分の行と枠を確かめる)
