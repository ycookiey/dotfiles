# Gate と test の設計則

CI/verify で走らせる自動 gate と unit/e2e test の設計で繰り返し踏む罠と一般則。audit v2 (2026-08) の 9 batch 実測から抽出。

## 1. codegen stale gate: git diff --exit-code 型に統一

**backup→regen→diff→restore 型は full verify で vacuous PASS になる**。

- 症状: gate 単体実行では diff 検出できるが、full verify に統合すると全 case で PASS (drift 検出ゼロ)
- 原因: verify の Phase 0 pre (or 相当の事前 codegen step) が先に in-place regen 済 file を作る → backup 対象が「最新値」= 再 gen 結果と一致 → diff が恒等的に空
- 正: `pnpm <gen コマンド> >/dev/null; git diff --exit-code -- "$GENERATED"` 形式。working tree が index と一致するか見るため Phase 0 pre が先行しても committed 状態との drift はそのまま diff に現れる
- 併せて gen script 側は write-if-changed (内容比較後の条件付き write) にする。drift なし時に mtime bump による cargo 全再 compile と並列 race を回避

## 2. test の non-vacuous 化: 「自己修復されない値」を pin

**assertion 値が観測後に他経路で正しい状態に上書きされる場合、その値の pin は vacuous** (fix を外しても system 全体が正しい状態に収束するため red にならない)。

例: cross-tool back e2e で「戻り先 URL が原 token のまま」だけを pin → 戻り先ページ自身が mount 直後の URL sync effect で正しい token を書き戻し、fix を外しても最終的に URL は元に戻る = vacuous。

対策: 誰も修復しない中間値を assert 対象に選ぶ。上記例では draft (sessionStorage) の中身を pin — draft は戻り先 mount で書き換わらない、fix を外すと clobber が残る形で確定的に検出可能。

一般化: fix 対象経路の効果が反映される「単一書き込み点」の値を pin する。多段 effect の最終値でなく、fix が唯一書く中間点。

同型: test 自身の teardown が判定対象を書く。判定は teardown 前の snapshot で行う。
- 実例 (Textual TUI): 「q キーで終了 → Bot の shutdown が呼ばれる」test。harness の後片付けが app 実行中なら `_shutdown_and_exit()` を呼び、それが shutdown の呼出記録を足すため、q が効いていなくても PASS。snapshot 化で q が効かないことが判明 (既存の制約)
- 確認: 判定値を teardown 前に copy、かつ「操作そのものの結果か」(app が操作で終了したか等) も併せて assert

## 3. fixture SOT 集約時: 既存 grep gate との衝突に注意

hardcoded fixture (e2e URL token 等) を SOT 集約する際、既存の grep 型 gate (check-*-freshness.sh 等) の pattern が「path と literal が同一行に隣接」を前提にしていることが多い。

例: `` `/foo?c=${TOKENS.combined}` `` テンプレ組み立てにすると `grep -rhoE 'foo\?c=[A-Za-z0-9_...]+'` が 0 件マッチ → gate は「nothing to verify」で silent PASS に落ちる。

対策:
- fixture 側は full-URL literal を保持 (`URL: '/foo?c=IMDT...'`) して既存 grep pattern を空振りさせない
- 新設 unit gate と既存 bash grep gate の役割分担を doc 明記 (前者は SOT 強制、後者は SOT 経由しない直書き復活の受け皿)
- SOT 集約 commit で必ず既存 gate が非空実行されることを実測 (BEFORE/AFTER の grep hit 件数比較)

## 4. property-based test の arbitrary は satisfies Record<keyof ...> 駆動

fast-check 等の arbitrary が state 型と手書き乖離すると、後付け field (v=N 内部再定義で追加された status enum、tdr 等) が arbitrary から欠落したまま「全入力空間から生成」の header 主張が陳腐化する。手書き round-trip test では 0 assertion 圏内に落ちて silent PASS。

対策:
- arbitrary を `satisfies ArbTable<Record<keyof StateType, ...>>` (or `satisfies Required<StateType>` の Omit 展開) で全 key 必須化 → state に field 追加した瞬間 typecheck fail
- per-field 保持 property は個別 test でなく `keyof` 網羅 loop に一般化
- enum は全 variant round-trip fixture (`assertAllVariantsObserved`) で pin

## 5. doc drift: 現行値 hardcode でなく SOT 相対化

**doc の「現行 X は :vN」型 hardcode は bump のたび drift**。checklist の「vN+1 に bump」も次の bump で古くなる。

drift 再発を構造的に排除する形式:
- 表構造: 「現行」行を廃し、**過去の確定値のみを by-batch 変遷として残す** (過去値は drift しない事実)。列見出しから「現行」の含意を除去 (例: `find-def history` → `bump 後 (find-def)`)
- 現行値の参照先: 「現行値は code 側 SOT (`<file>` の `STORAGE_KEY`) を見よ」と明記。「最終行は現行値ではない」sentinel を付与
- checklist: 「vN+1 に bump」→「現行 STORAGE_KEY を +1、旧 key を legacy 配列へ追加」の相対表現
- 衝突警告: legacy 削除リスト等がある場合、「legacy 配列に既にある値を新 key に選ばないこと」を 1 行併記 (毎 load 削除の実害経路を残す)

同型の drift 経路: version-hardcode API doc、magic number コメント、hard 相対 path。SOT を明示 + 相対参照 + 変遷履歴のみ残す形が汎用。

## 6. codegen 生成物と fmt gate の両立: template SOT に `#[rustfmt::skip]` 出力

Rust codegen で生成する file が長い列挙 (id の `|` 列挙 40 個超等) を含むと、rustfmt 折返し要求と stale gate (SOT 再生成結果と一致) が競合。fmt green にすると stale red、逆も然り。

- 対処: template SOT (`packages/*/scripts/gen-*.ts` 等) 側で生成関数直上に `#[rustfmt::skip]` (関数単位) / `#![cfg_attr(rustfmt, rustfmt_skip)]` (module 単位) を明示的に出力
- 理由: 生成 file への手作業 attribute 追加は次回再生成で消える。stale gate は SOT 再生成結果一致を要求、SOT が唯一の style 出力源
- 再発リスク: 現状 SOT が短くて顕在化していない他 gen file も列挙数増で同型再発。予防的 `#[rustfmt::skip]` 出力が安全
- 検知: `cargo fmt --check` と `check-*-stale.sh` を同一 lane 並走で fmt fix apply 直後に stale red 露出

## 7. lint gate 実効化と bulk operation の順序制約: 既存違反を lefthook が拾う

lefthook `{staged_files}` は「staged file 中の既存違反も拾う」= 違反の既存/新規を区別しない。lint gate 実効化 commit 後に一括 refactor / format 等 bulk commit を試みると、staged 中の既存違反 file で block、以降 commit 全凍結 (他 agent 含む)。

- 順序: (1) gate 実効化 commit → (2) 既存違反一括修正 (auto + 手作業) を即座連続 apply → (3) その他 bulk operation
- gate 実効化 + fix commit の連続 landing を段取り、分離 sub-batch でも 1 セッション内完結
- fix を次 batch 以降に defer する場合は dev loop 凍結を承知の判断

## 8. 並走 verify で共有 log/status dir が summary を破壊 = 偽グリーン化

固定共有の log/status/output dir を持つ script は複数 run 並走で後発が先発 file を破壊し、fail gate が消えて「pass のみの偽グリーン」化。false green class の一種。

- 検知: 同 script を 2 run 並走 → 先発 summary の pass/fail 件数が過少集計になるか実測
- 対処: `LOG_DIR=<base>/run-$$` に分離 + `<base>/<name>` mirror で既存参照互換 (`<base>/<name>.log` 経路維持) + 走行中 run を `kill -0 <pid>` で FIFO 削除対象外
- symlink/junction は Windows `rm -rf` で実体消す罠回避で不採用、root mirror + copy が汎用
- --parallel-safe や multi-agent 前提の script は特に対処必須

## 9. 再生成型 gate script の tree 汚し = backup/restore 汎用 pattern

「SOT から再生成 → diff 判定」型 stale gate は実行後 tree に副作用を残し、他 agent の verify がその汚れた state を掴む。gate 追加時 checklist として「実行で tree に副作用が残るか」を確認。

- 汎用 pattern: (1) 実行前に対象 file を backup (scratchpad or `/tmp` cp) → (2) 再生成 → diff 判定 → (3) EXIT trap で backup から restore (成功/失敗どちらでも)
- skip-worktree 対象なら S/H/untracked 3 分岐、通常 tracked file なら H のみで足りる
- 適用実績: check-known-moves-stale (skip-worktree 対応)、check-third-party-notices-stale (通常 tracked)
- 再発リスク: rustfmt 型 gate (damage_ids_gen / variable_power_gen 等)、snapshot gate (URL_POKEMON_FORM_INDEX 等) も対象

## 10. turbo/nx/bazel の env 経由 cache invalidation 非空振り実測 = deadbeef-probe 3 段

turbo/nx/bazel 等 cache 系ツールに env 経由で invalidation を効かせる時、宣言 (`env: [...]`) と env 露出 (shell/CI) の両方が揃わないと vacuous (常時 cache hit or 常時 miss)。片方だけだと sinkhole。

- 非空振り実測 3 段:
  - run 1: 実 hash → cache miss + executing (base line)
  - run 2: 同 hash → cache hit + replaying (cache 効いてる確認)
  - run 3: **deadbeef-probe** (fake hash injection) → cache miss + task hash 変化 (env が cache key に効いてる非空振り確認)
- run 3 が cache hit になったら宣言か env exposure のどちらかが欠落
- 適用実績: WASM_INPUT_HASH = wasm-pkg/.input-hash content を turbo.json + verify.sh + ci.yml で 3 点セット

## 11. codegen は言語境界必要時のみ、同 language 内は runtime table

codegen (wasm-layout / rust bindings 等) は言語境界 (Rust struct → TS 等) or SOT/consumer の実行時分離が必要な場合の必要悪。同 language 内 SOT/consumer なら runtime const array で対称性が構造的に保証される (走査 array 共有) = 「検査 (stale gate) より強い設計」。

- 判定: SOT と consumer が同 language + 同 process か
- 同 language: runtime `const` 配列宣言、encode/decode が同 array を走査 → 対称性構造的保証
- 言語境界: codegen boilerplate 必要 (generated file + stale gate + regen script)
- 追加 gate より、そもそも壊れない設計を優先。stale gate は「検査 mechanism」= runtime table は「check 不要な設計」
- 副次: v=N snapshot による migrator が「const 配列を migrators/ へコピー」で済み、explicit migrator 実装コスト低減

## 12. property/roundtrip test の wire format 盲点 = layout signature pin

property/roundtrip test は encode/decode が同時にずれると対称性が保たれる → **wire format 変更 (field 順序入替 / bit 幅変更 / semanticMax 変更) に原理的盲目**。table-driven / codegen で対称 encode/decode を採用したら、以下の layout signature pin を必ずセットで導入。

- 期待値: `key:bits<=semanticMax` の順序付き literal を test に文字列 pin
- 幅 / 順序 / field 増減 / semanticMax 変更を 1 つの期待値で全捕捉
- codegen + `git diff` stale gate でも同じ盲点 (encode/decode 両方同時に regen → symmetric、fixture token も再生成で自己一致) = pin 方式が唯一の正解
- 実測: injection「field 順序入替」で property 全緑 + signature pin 赤化を確認

## 13. process-global state を持つ system の bench = 宣言順走行で state 継承

bench 単独実行と全体実行で異なる結果を出す silent 空振り class。原因: process-global state (module scope singleton / worker state / cache) を無条件上書きする init が bench 宣言順で走ると、前 bench の state が後 bench に継承される (全体 vs 単独で ~3.5x 乖離実測例あり)。

- 対処: **bench body 冒頭で自 init を自己完結化** (`beforeAll` は不足、bench ごとに前 bench state で汚染される)
- 前提 fixture の要件 (roster N 以上等) は module load 時 throw で構造的検知 (`it()` でなく module scope、vitest bench mode は `it` 実行しない)
- bench 名に実体数を埋め込み (「(典型) find-def-phase1 (201体)」等) で乖離を人間可読化
- vacuous 検知: 単独実行と全体実行で mean 値が有意差 (~2 桁以上) なければ空振り疑い

## 14. pnpm.overrides の selector 分離 pin は残存 tree に効かない = blanket 統合

pnpm.overrides の CVE pin (`"name": ">=version"`) は selector 分離 (`"name@<X.Y.Z"`, `"name@2"` 等) 経路の残存 tree に効かない。version bump しても既存解決が新 range を満たすと再解決されない仕様。

- 対処: 汎用 pin は blanket (`"name": ">=version"`) が正、selector 分離は特定 tree 経路の版縛り目的でのみ
- 実測: `pnpm audit --prod` で GHSA 消滅 + `pnpm why name` で単一 version 化
- 前提: overrides 変更後 `pnpm install --frozen-lockfile=false` (lockfile 再生成) 実施しないと新 pin 未反映
- 陥りやすい罠: 「pin したのに audit red」= selector 分離経路の残存を疑う
- **major boundary 越えの CVE pin は不可**: 例 `">=3.3.18"` は latest major (nanoid 6) に解決 = 破壊 (postcss CJS require が ESM 6 系で壊れる、V2-23 実測)。同一 major range 内は blanket 統合が正、major boundary 越えは caret (`^X.Y.Z`) or workspace direct bump
- 判定: `pnpm view <name> versions --json | tail` で latest major 確認、consumer 破壊しないか実測

## 15. interface / abstract method の「実装済み production caller 0」は knip 盲点

interface `method(): T` は「型定義」であり関数 export ではないため、knip 等の unused export gate は「未参照 method」を検知不能。「実装済み caller 0」pattern が silent に埋もれる。

- 検知: 多 method Port (CalcRuntime 等) を追加/削除する時は「全 method の production caller 数」を grep 手動棚卸し (`grep -rn 'runtime\.methodName' src/`)
- V2-17 実発生: guard 撤去で唯一 caller が消えた `findDefCombined` / `searchAttackersPhase2` が interface member のため knip すり抜け、手動 audit で発見
- 予防: Port method 追加時 checklist に「production caller が存在するか (test/storybook のみは caller とみなさない)」項目
- 副次: 実装 (concrete class method) は unused method として lint で拾える場合あり (rule 設定次第) だが、interface 側は type 定義扱いで検知不能

## 16. gate 盲点 pattern: 対象範囲外の依存 (型 only / 未選択 feature / cfg 外)

gate は「対象範囲」で決まる = 型のみ依存 / 未選択 feature / cfg 外 block は構造的に盲点。

- **depcruise `import type` 越境検知盲点**: `import type` は module graph に載らない → `hexagon-no-adapters` 等 rule で検出不能。値 import で切り替えて発火。実インスタンス: `params-to-wasm-input.ts:52` の `import type { DamageInputRaw } from "../wasm-serialize"` (Adapter 登録済 file への型 only 依存が現存)
- **lint gate feature 走査盲点**: `#[cfg(feature = "X")]` block 内のみで使う import が feature off の lint gate で未使用に見える (V2-14 で削除された import が V2-24 で E0425 として発覚)
- 対処: depcruise は `dependencyTypes: ["type-only", "aliased-type-only"]` 含める / lint は multi-feature matrix 走査 / import 側は完全修飾 or `#[cfg]` 付き
- checklist 追加: gate 追加時に「対象範囲外の依存を持つコードがあるか」確認

## 17. probe injection の空振りリスク

anti-vacuous probe injection 自体が空振りするリスクあり。lint rule の発火条件 (crate-type / cfg / active lint set / target 依存 rule) を実測で確認してから採用。

- 実例: packages/wasm の wasm32 lint lane probe に `#[cfg(target_arch="wasm32")] fn dead() {}` 注入したが crate-type=["cdylib","rlib"] で `dead_code` lint 発火せず = lane 検証自体が vacuous 化
- 対処: 「発火するはず」でなく「実測で 1 度発火させた」rule のみを probe に使う (V2-23 で `needless_return` 使用)
- probe 設計 checklist: (1) 対象 rule が active か / (2) crate-type / cfg / target で発火条件を満たすか / (3) 現状 code base で 1 度発火することを実測

## 18. Rust cfg(test) / cfg(bench) の外部 crate 特性

`cfg(test)` は「lib 自身の --test 時」限定、外部 crate (bench / example / tests/) では効かない。

- 仕様: `cfg(test)` は lib crate 自身の `--test` build 時のみ true。bench / example / `tests/` は外部 crate として通常 build lib に link → 内部 `cfg(test)` は false
- `cfg(bench)` は stable Rust 不在 (`#![feature(test)]` nightly か `RUSTFLAGS=--cfg bench` 手当のみ)
- **`#[cfg(any(test, bench))] pub fn` は目的達成不能** (bench から見える visibility は依然 `pub(crate)`、E0624)
- 外部 crate から見せたい: pub 化 + safe wrapper (`try_from_buffer` 等) が正、cfg 経路は不成立
- lib 内部 test のみ visibility 開放は `cfg(test)` で OK

## 19. spec scope drift 検知: 専用 config + assertion ゼロは default suite 除外必須

専用 config (`playwright.perf.config.ts` 等) を持ち assertion ゼロの計測 spec は default suite から除外必須。testIgnore SOT の whitelist 撤廃時に移行漏れが発生。

- 実例: V2-13 whitelist 撤廃時に `find-def-combined-worker-shard-e2e.spec.ts` が testIgnore 未登録で default 混入、CPU 逼迫で worker init 15s 超過 = V2-25 で発覚
- 検知 gate 候補: 「専用 config の testMatch に含まれる spec が default config の testIgnore にも含まれる」を機械 check
- 代替: CI で「default suite の各 spec に assertion がある」を lint (assertion ゼロ spec は default 対象外)
- symptom: default suite で CPU 逼迫 → worker init / dev server 起動 timeout の flake

## 20. Next.js App Router (15.x) history.replaceState race + app-side mitigation

Next.js App Router (15.x) で `history.replaceState` に同 URL を渡すと router transition を積む = 見た目 no-op でも race 火種になる。HistoryUpdater が router state 変更時に無条件 replaceState、popstate と race で戻り先 entry を上書き。

- 対処 pattern: history API wrapper で「replace の結果 URL が現在 URL と完全一致なら history API 呼ばない」skip 追加
- 決定論的因果特定: 「同 URL replaceState → back()」を人工実装で 100% clobber vs 対照 0% 分離
- prod 側実測必須: `output: 'export'` 相当で trailing slash 有無を確認 (path 比較の一致条件が変わる)
- 根本原因は Next.js 15.x 内部 (startTransition + popstate 順序保証なし)、app 側 mitigation のみで完全 fix 不能 = no-op でない replace は race 依然

## 21. fixture drift 修正時の signal 保持 (章 2 の逆側)

fixture drift 修正時、`as any` / assertion 削除 / cast 追加は「signal を消す」= NG。真値 (seed-data / 実 API) から補完が正。silent skip / TypeError 経路が露呈するなら「gate が壊れていた」証拠、gate 側を先に直す。

- 実例 (V2-24 F7 106 error 修正):
  - `makeLoadoutWithNature` 3引数 → 8箇所で第4引数 silent 捨てられ = 特性/持ち物 cross-validation が特性なし同士に縮退 (probe で `45→88` 実証)
  - `hits vs rolls` 型不整合 = try/catch で cache-miss 全 skip の露呈
  - `aPt vs requiredPt` + solutions 0 loop 空 = 完全空振り
- 手順: (1) drift 修正前に「なぜ silent PASS していたか」の signal を probe で顕在化 / (2) 真値補完で修正 / (3) probe で「今の gate が signal を捕まえる」逆証明
- 章 2 (non-vacuous 化) の逆側 = drift 修正 = signal 復元

## 22. 検証の照合先 = 実装と独立した外部基準

「合っているか」の確認で、同じ関数/同じ入力を共有する 2 値の一致を根拠にしない = 両方同じ誤りで一致する (偽グリーン)。照合先は実装と独立: 実物・user の観測値・別経路の計測。

- 実例 (画面共有の切り落とし見本): 見本の寸法と録画サイズが一致 → 「合っている」と回答。両方とも同じ `cropRect` に縮んだ取り込みを入力 → 一致は当然。user は実物 (1920×1080 のゲーム・モニタ高さ) 基準で 1866×1050 の誤りを指摘。真因は取り込み時点の縮小
- 手順: (1) user が何を基準に問うているかを先に特定 / (2) その基準と同じ外部値を取得 (実物の寸法・別 tool の計測) / (3) 取得不能なら「内部一貫性のみ確認、外部基準は未確認」と明記
- 章 2 (non-vacuous) の検証版: assertion の期待値が実装由来なら空振り

## 23. test 移植 (app→library 等) の同等性 = assertion 単位で照合

移した test の「対応表」を題名・件数で数えて完了とするのは vacuous。題名が対応しても中身は別物になりうる。

- 実例 (app の保存処理を library へ移植): 対応表 91 件を「同名の test が移植先にあるか」で数え全件 OK → assertion 単位で照合し直すと 同じ 14・部分的 68・逆の振る舞い 9。原因: 移植先 test が保存を記録しない stub 相手 (「何も保存しない」「保存された中身」を確かめられない)・結果を `kind` だけ比較・in-memory 実装が履歴を持たず確かめようがない
- 手順: (1) 元 test の assertion を 1 つずつ列挙 (戻り値の形・保存された値・保存されない事実・版・時刻・履歴・event・順序) / (2) 移植先で同じ assertion を書く。保存結果は stub でなく実装 (in-memory 実装可) から読み戻す / (3) 照合は別 agent、差の報告は自分で code を読んで確認 / (4) 実装を故意に壊し (mutation) test が落ちるか確認
- mutation の偽 caught: 壊し方で build 自体が失敗すると、build 成果物 (dist) 経由で import する test が読込で落ち「捕まえた」と誤判定。build が通る形で壊す (未使用 import を残さない等) か、src を直接読む test に限定
- agent への完了条件は成果物の数でなく振る舞いの数 (assertion の対応・mutation 全数 caught)。章 2 (non-vacuous) の移植版
- 照合の設定 = 移し元の実際の設定: 移し先に既定で無効の規則・設定があるとき、移し元が強制していた規則ごとに、移し元の実際の設定 (定義・config) で移し先の入口 (route 等) を通す test を書く。移し先の見本の設定での test だけでは、設定の入れ忘れを捕まえない
  - 実例 (大会 app → kit): 移した test は kit の見本の定義 (「1 人 1 チーム」有効) で書き全件照合済み。app 自身の定義に kit の `uniquePlayers` を入れ忘れ (kit の既定は無効)、どの test も捕まえず、試験用サイトで 1 日弱 重複の申請が通った

## 24. 数・有無の一致で「全部出た」と結論しない = 中身を 1 対 1 で照合

表示・出力の確かめで、件数や有無の一致を「全部正しい」の根拠にしない。同じものが 2 か所に出ても数は合う (重複と欠落が打ち消す)。

- 実例 (配信画面の録画とログ): 枠ごとのアイコンの有無と覚醒の数がログと全区間で一致 → 「全員出た」と記録。アイコンの種類を画像と突き合わせると、別チームの同名の 2 人に同じ選手の装備が出て、1 人は出ていなかった
- 手順: (1) 数の一致は「数は合った」までに留めて書く / (2) 結論の前に中身 (種類・誰のものか) を 1 対 1 で対応づける (重複を許さない割り当て) / (3) 対応づけられないなら、その旨を結論に書く
- 章 22 (照合先の独立) の粒度版: 照合先が独立でも、比べる量が粗いと偽グリーン
