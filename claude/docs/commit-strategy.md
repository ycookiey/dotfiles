# Commit 戦略: fixup vs 新規 commit の要素別判定

複数変更を同時に commit する際、各要素を fixup / 新規 commit のどちらに載せるかを**要素別に毎回判定**する。反射で決めない。

## 判定基準

各要素について性質を評価:

- **skeleton / 既存 feat commit の見落とし・bug** → **fixup 候補**
  - sideEffects hint 抜け、tsconfig 誤設定、publish surface 欠落、silent breakage リスク等
  - fixup 先: 見落としが本来入っていた commit(skeleton commit or 該当 feat commit)
- **既存 commit の意図追加**(feat commit が実装した機能への refinement) → **fixup 候補**(その feat commit へ)
- **新機能 / 別意図の変更**(次 phase の feature、consumer feedback 後の追加、明確に後発の要件変更) → **新規 commit**
- **境界曖昧**(skeleton で意識されて然るべきか、後追いか、両方の見方が成り立つ) → **skeleton fixup 側に寄せる**
  - history が「skeleton は最初から well-configured」に見える方が読みやすい

## 陥りやすい罠

- 「これは新機能っぽい」と反射で新規 commit を選ぶと、本来 fixup で吸収できた見落としが history に露出し blame 煩雑化
- 何でも fixup すると、意図的な後追い追加が別 commit で読める文脈を失う
- **要素別に評価**すれば両方回避

## 適用手順

1. 変更をカテゴリ別に分ける(build 設定 / metadata / bug fix / 新機能 等)
2. 各カテゴリで性質判定(見落とし / 意図追加 / 新規)
3. fixup 候補は fixup 先 commit を特定(`git blame` / `git log --follow`)
4. 新規は commit message で意図を明示
5. 実施前に user と分割案を確認(特に境界曖昧なケース)

## 具体例(publish 前準備、3 package の 6 種変更)

- (A) `sideEffects: false` 3 package: 初期化見落とし → skeleton fixup
- (B) `engines` を統一(1 package のみ既存): 3 package 統一の見落とし → skeleton fixup
- (C) `prepublishOnly` を統一: 同上 → skeleton fixup
- (D) `repository` / `bugs` / `homepage` / `keywords`: 一見「publish 準備の新規」だが「skeleton 段階で入っていて然るべき」とも見なせる → skeleton fixup(境界曖昧を fixup 側に寄せる原則適用)
- (E) `tsconfig.build.json` 分離 + build script 切替: test 混入 bug の修正 → skeleton fixup
- (F) tsconfig の誤 compiler option 削除: skeleton 誤設定 → skeleton fixup

→ 6 種類を 1 つの `fixup!→<skeleton commit>` に統合、history クリーン維持。

## fixup 拒否ケース

明確に「skeleton には無かった意図の追加」:
- 新 feature の追加
- consumer feedback で追加した機能
- 明確に後発の要件変更

→ 新規 commit で意図を残す。

## 関連

- `git.md`: git operations、rebase / autosquash の罠
- CLAUDE.md「commit ルール」: AI ツール名 / Co-Authored-By 禁止
