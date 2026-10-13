# Office ファイル(pptx・docx)を生成するときの罠

スクリプトで pptx・docx を作る・書き換えるときに、見た目が崩れる・開けなくなる原因と対処。

## pptxgenjs のフォント

- `pres.theme = { headFontFace, bodyFontFace }` はテーマの英字用(`<a:latin>`)だけを決める。日本語用(`<a:ea>`)は空のままで、日本語は `<a:font script="Jpan">`(游ゴシック)に回る
- テーマの参照(`+mn-lt`・`+mj-lt`)は、ONLYOFFICE などテーマを読まないアプリで Arial になる(2026-10 実測: ユーザーの ONLYOFFICE で本文が Arial)
- グラフは、`*FontFace` を渡さない所に pptxgenjs が `Arial` を書く(タイトル・数値ラベル等。非表示の数値ラベルも含む)
- 対処: 文字ごとに直接書く
  - `addText` の既定の options に `fontFace` と `lang: "ja-JP"`(英字用・日本語用の両方に入る)
  - レイアウトのプレースホルダの options、`slideNumber`、グラフの `titleFontFace`・`dataLabelFontFace`・`catAxisLabelFontFace` 等の全部
  - テーマの `<a:ea typeface="">` と `<a:font script="Jpan">` は、`applyTheme` の後に JSZip で `ppt/theme/theme*.xml` を書き換える
  - 確認: 展開して `grep -o 'typeface="[^"+]*"' ppt/slides/*.xml ppt/charts/*.xml` で残った Arial を数える(マスターの `buFont` の Arial は箇条書きの記号用なので無害)
- 太さを変えられる 1 ファイルの可変フォント(例: NotoSansJP-VF)は、LibreOffice の PDF でフォント名が `-Thin` になるが、描画の太さは正しい。名前だけで判断しない

## python-pptx でスライドを消す

- `sldIdLst` から外し `drop_rel` するだけでは、セクションの一覧(`p14:sectionLst` の `p14:sldId`)に消したスライドの番号が残る。PowerPoint が修復を求める原因になる
- 対処: 残したスライドの id だけを残し、空になったセクションも消す

## 日本語の docx の行間

- 日本語の様式は、本文の行の位置が文書のグリッド(`w:docGrid` の `linePitch`)に合わせられ、段落の `w:spacing` の `line` を小さくしても行間が変わらない
- 対処: 足す段落の `w:pPr` に `<w:snapToGrid w:val="0"/>` を入れてから `w:spacing` を付ける

## 確認

- 見た目は LibreOffice で PDF にして画像で見る(`soffice --headless --convert-to pdf` → `pdftoppm`)。ページ数が様式どおりかも数える
- 行の最後の 1〜2 字だけが次の行に落ちる所は、フォントを替えると増える(字幅が違う)。替えたら全ページを見直す
