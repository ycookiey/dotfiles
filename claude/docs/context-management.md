# Context 管理

長寿命 reference になり得る tool 出力・agent report は、context 圧縮で失う前に永続化する。

## Agent 出力の永続化

reviewer / researcher / general-purpose 系 subagent の返す report は多くの場合 30 行超の詳細(findings list、code 引用、file 位置、trade-off 分析)を含む。**受領直後の scratchpad ダンプ**が原則。

### タイミング

- 受領: agent から report 到着した瞬間
- ダンプ先:
  - session scratchpad (`$SCRATCHPAD/<topic>.md`): session 限定、圧縮対策のみ目的
  - 対象 project の docs (`<project>/docs/reviews/vNN-report.md`): commit 対象、後の session でも参照
- 内容: **要約でなく元の詳細**(findings ID・優先度・file:line・failure_scenario・改善案 の 5 要素は必須)

### タイミングを外すと失う

反映作業を始めると tool call が積み上がる。**~20 call 超えで context 圧縮 trigger の可能性**があり、それを跨ぐと未消化 finding の詳細が失われる。特に multi-item deliverable(数十 findings)は反映途中で圧縮確実。

### 失敗事例

- v17 review(HIGH 10 / MEDIUM 33 / LOW 46)受領 → Fix + Finding 混在で ~40 tool call 消費 → 圧縮発生 → 残 MEDIUM 22 件の詳細を失い、追加レビュー再起動を余儀なくされた
- 教訓: reviewer 出力を受領した瞬間に file write、それから反映開始

## 判断基準: 何を永続化するか

以下のいずれかに該当 = 永続化候補:

- 30 行超の agent report
- Multi-item deliverable(finding list、TODO list、比較表など)= 反映 tool call が予測できないもの
- 過去 web fetch した外部 spec / 論文 excerpt(再取得コスト大)
- Cross-session で共有したい状態記録
- 「後で見直す」と自分が判断した任意情報

以下は永続化不要:

- 単発の bash 出力(grep 結果、git status など)= 再実行で復元可能
- File Read 結果 = harness が state を track、再 Read 可能
- 即座に消化する 1 finding = 反映後は失っても影響なし

## 反映と永続化の順序

1. Agent report 受領
2. **受領直後**に scratchpad へ file write(順序を逆にしない)
3. Reflection / prioritization
4. 個別 finding 反映(commit ごとに `git add file:line` 明示)
5. 反映後、scratchpad の該当 finding に status update(fixed / skipped / deferred)

status update しないと「何が残っているか」不明 = 追加 review 再起動の原因になる。
