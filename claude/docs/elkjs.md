# elkjs(ELK layered)

確認版: elkjs 0.12.0

## 入れ子(`elk.hierarchyHandling: INCLUDE_CHILDREN`)の罠

- 子ノード(枠)の `layoutOptions` に `considerModelOrder` / `forceNodeModelOrder` / `cycleBreaking.strategy: MODEL_ORDER` を渡すと `Cannot read properties of undefined (reading 'a')` で落ちる → 子には spacing / padding だけ渡す
- ルートに渡しても、入れ子では循環のどの矢印を右から左へ描くかが定義順どおりにならない → 戻る矢印は向きを反転して ELK に渡し、描くときに点列を反転して戻す
- 座標をキャンバス基準で受け取る: `elk.json.shapeCoords` と `elk.json.edgeCoords` を `ROOT`(既定は親基準)

## ラベル

- エッジのラベルを線の上に置く `elk.edgeLabels.inline: "true"` は、ラベルごとの `layoutOptions` に書く。ルートに書いても効かない
- ノードの外(上など)にラベルの場所を確保する: ノードの `layoutOptions` に `elk.nodeLabels.placement: "[H_LEFT, V_TOP, OUTSIDE]"`、ラベルに `width` / `height`。エッジはノード本体につながる。ノードとの間隔は `elk.spacing.labelNode`
- ELK は文字を測らない。ラベルの `width` / `height` は呼び出し側の見積もり(拡大表示する文字なら最大の大きさ)
