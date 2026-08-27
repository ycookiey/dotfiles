# 補助スクリプト + agent prompt テンプレート

修正ワークフローで使う補助スクリプトと、agent 起動用の prompt テンプレート。

## agent prompt テンプレート

agent (general-purpose) に渡す prompt:

```
あなたは日本語文書のレビュアー。以下の文書を読み、AI生成テキストや学術過剰な表現・英語統語調・読みにくさを平易化する候補を抽出してください。

# 対象ファイル
<file path>

# チェック観点 (17観点)
(1) 学術漢語 / (2) 漢字連続 / (3) カタカナAI臭 / (4) AI生成典型構文 / (5) 内輪言及 /
(6) 記号・マークアップ / (7) 事実 vs 解釈 / (8) 口頭発表・原稿 / (9) 動詞・助詞 /
(10) 構造化 / (11) 和欧混在スペース(プレーンテキスト時のみ) / (12) リズム /
(13) 予告→凡庸の落差 / (14) 心理誘導 / (15) 具体性なし断定 /
(16) 英語統語・翻訳調 / (17) 読みやすさ原則

観点ごとの詳細は skills/refine-japanese-prose/references/ 配下参照:
- (1)(2)(3) → vocabulary.md
- (4)(13)(14)(15) → ai-patterns.md
- (5)(10) → structure.md
- (6) → markup.md
- (7) → fact-interpretation.md
- (8) → speech-manuscript.md
- (9) → verb-particle.md
- (11) → spacing.md
- (12) → rhythm.md
- (16) → translationese.md
- (17) → readability.md
- 文書タイプ別必須要素 → doctypes.md
- ジャンル別 override → genres.md

# 出力形式
各候補について:
- 該当箇所(節/原文)
- 問題カテゴリ((1)-(17) のいずれか)
- 推奨修正案(複数候補可)
- 優先度(高/中/低)
- 維持判断のもの は別カテゴリで出力

# 注意
- 固有名詞 / 専門用語 / 直接引用は触らない
- 全観点を全箇所に一律適用しない(スイープ改稿の罠)。「変える価値がある」箇所を選ぶ
- 生の痕跡(口語引用、表記の癖、節ごとの不揃い)は人間らしさの資産。標準化しない
- これまでの修正履歴があれば伝えて、重複指摘を避ける

# 既に適用済の修正 (重複指摘しないこと)
[累積した修正履歴を列挙]

# 判断台帳の残す判断 (再指摘しないこと)
[前周までに「残す(理由)」に仕分けた finding を列挙]

# 出力先
<output path>
```

## PDF生成 (学術レポート用、pandoc + lualatex + luatexja)

日本語と英字の間の自動アキを消す YAML フロントマター:

```yaml
---
header-includes: |
  \ltjsetparameter{xkanjiskip=0pt}
  \ltjsetparameter{kanjiskip=0pt plus 0pt minus 0pt}
---
```

pandoc コマンド例:
```bash
pandoc input.md -o output.pdf \
  --pdf-engine=lualatex \
  -V documentclass=ltjsarticle \
  -V geometry:margin=2.5cm \
  -V fontsize=11pt
```

## ja-prose-lint 集計

```bash
python -m ja_prose_lint <file> --output result.json
python -c "
import json
from collections import Counter
data = json.load(open('result.json', encoding='utf-8'))
findings = data['findings']
print('high:', sum(1 for f in findings if f['severity']=='high'))
print('medium:', sum(1 for f in findings if f['severity']=='medium'))
print('low:', sum(1 for f in findings if f['severity']=='low'))
print()
print('detector別:')
for d, n in Counter(f['detector'] for f in findings).most_common():
    print(f'  {n:3d} {d}')
"
```

- high(主に pos_errors)はノイズ多めだが、`continuous_kanji` `repeated_phrases` は有用
- 数百字の単一段落など短文では手作業レビューで足りる(スキップ可)

## 文字数計測(改行除く)

PowerShell:
```powershell
((Get-Content f -Raw) -replace "\r|\n","").Length
```

学術レポート等の文字数上限がある文書で、平易化(漢語→和語で字数増)と冗長削除(括弧注解体で字数減)の残り字数を実測する。

## 将来的な拡張候補 (ja_prose_lint 未対応)

- 文長 CV、段落あたり文数の分散
- 体言止め有無
- 文頭反復頻度
- 「〜ではなく」頻度の検出(観点(4))

現時点では agent 側で検出する。
