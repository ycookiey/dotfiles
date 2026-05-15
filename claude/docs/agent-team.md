# Agent Team: Leadオーケストレーションガイド

## 原則・ルール

- Leadは委譲・統合・判断のみ。実装・調査・レビュー・テスト・commitは一切しない。失敗しても再委譲
- implementerには実装完了後のcommitを必ず指示する
- 独立タスクは複数member同時起動。待ち中も別タスクの準備・起動・統合を並行検討
- 同一ファイルの同時編集禁止
- 完了タスクは削除せず記録として残す
- Leadのcontext最小化を意識

## 手順

1. **session prefix発行**: `TeamCreate`直後にBashで `date +%y%m%d-%H%M%S | cut -c1-8` 等を実行し、5-8文字程度の短いprefixを得る(例: `s60503a`)。以降のtask-idは必ず `<prefix>-T-1` 形式とする(`T-1`単独で使わない)
2. `TeamCreate` → `TaskCreate`（id=`<prefix>-T-1` 等。依存: `blockedBy`。同一ファイル編集も依存に含める）
3. `Agent`でspawn（`team_name`, `name`, `subagent_type`指定）→ Lead自身が`TaskUpdate(owner=<member>)`設定。spawn promptに`TaskUpdate(owner=...)`は書かない(memberが自分にownerを立てるとharnessが自己宛 `task_assignment` 通知を発火し、完了報告後に重複報告の無駄動作になる)
4. memberの`SendMessage`を受けて統合・次指示。`blockedBy`解消済みの未着手タスクがあれば即spawn（ユーザ確認不要）
5. 完了後 `SendMessage`で`{type: "shutdown_request"}`

session prefixの目的: 複数Claude Codeセッションが同一repoでagent teamを動かしたとき、worktree(`agent-<task-id>`)と`.agent-output/<task-id>/`が物理衝突して同じファイルを取り合うのを防ぐ。同一タスクの意図的reuseはprefix固定で従来通り可能。

## フロー

researcher → planner → implementer → reviewer（並列可）
- 要件が明確で小規模: implementerから
- 設計判断・要件整理が必要: plannerから
- 前提知識・影響範囲の調査が必要: researcherから
- テスト作成が必要な場合:
  - plannerあり（仕様明確）→ TDD: tester → implementer
  - plannerなし（小規模・バグ修正）→ 後追い: implementer → tester

## 成果物ファイル

詳細が多い場合は `.agent-output/<task-id>/` にファイル出力し、SendMessageでは概要+ファイルパスのみ送る。Leadのcontext肥大化を防止。
- 例: `.agent-output/s60503a-T-7.1/plan.md`, `.agent-output/s60503a-T-7.1/research-api.md`(task-idはsession prefix込み)
- implementerへはspawn時にファイルパスを渡す

## Spawn

- 前memberの出力サマリー（またはファイルパス）、期待出力形式、「やらないこと」の明示
- パス指定: prompt内の絶対パスはLeadが組み立て時に`<WORKTREE_ROOT>`(agent-spawn-prep.sh stdout)を直接埋め込む。main repo pathをそのまま書くとworktree外参照になる。spawn-prep.shはpath書き換えを行わない(詳細はurleader SKILL.md)
- ブロック時: memberがSendMessageで報告 → Leadが追加情報or別member

### planner: plan構成指示

spawn promptの `plan_layout: single|multi|auto` で構成を指示(省略=auto)。
- `single`: 単一plan file
- `multi`: 複数file(`00-overview.md` + `NN-<phase>.md`)。phase一覧も指示可
- `auto`: planner判断

plannerが規模乖離を検知すれば報告するので、Leadが再判断。

## Worktree運用

書き込みsubagent(implementer/tester)のspawn時、SKILL.mdの「Worktree Isolation」手順に従う。
- prompt file作成 → `~/.claude/skills/urleader/scripts/agent-spawn-prep.sh` 実行 → 書き換え済みpromptでAgent spawn (merge-backも同ディレクトリ)
- subagentにはWORKTREE_GUARD_ROOT環境変数を設定させる(補助。hookはconfigファイルから自動解決)
- `.agent-output/` への書き込みはEdit/Write toolを使用(Bash redirect不可)
- cleanup/merge-back: 完了後は `agent-merge-back.sh --task-id <TASK_ID>` でmainへ取り込み+worktree/branch削除を一括実行
- 例外: researcher/plannerは読み取り専用のためworktree分離不要

### merge-back挙動

`agent-merge-back.sh` はworktree内で `git rebase main` → main側で `git merge --ff-only` → worktree remove + branch -D を行う。並列agentでmainが進みdivergeした場合も自動解消。conflict時は `rebase --abort` + 非0 exitでworktreeが保持されるので、Leadは上流memberに再指示するか手動対処する。手動cherry-pickは応急処置にとどめ、原則本scriptを使う。pushはしない。

### ファイルの自動コピー(allowlist方式)

`git worktree add`はtrackedのみ持ち込むため、`.agent-output/`や`.env`等のuntracked/ignoredファイルはworktreeから見えない。agent-spawn-prep.shが以下のallowlistを読み、tracked**でない**ファイル/ディレクトリ(=untracked or ignored)をworktreeへコピーする。trackedは既にworktreeにあるため除外(allowlistに誤って書いても上書きされない)。

- global: `~/.claude/worktree-copy.list` (dotfiles管理。既定で`.agent-output/`, `.env*`等)
- project: `<repo>/.claude/worktree-copy.list` (プロジェクト固有を追加)

書式: 1行1パターン(repo root相対のglob)、`#`コメント、空行無視。両方読みunion。

#### allowlist 行 syntax

行は空白区切り token として解釈。`@key:value` 形式はディレクティブ、それ以外の 1 token を pattern とする。順序自由。

- `@junction:<pattern>`: NTFS junction でmain と worktree を共有 (下記)
- `@tag:<name>`: 条件付き行の opt-out (下記)
- 残りの token は glob pattern (1 行 1 個まで)
- `#` 以降はコメント

例:
```
.agent-output/                                # plain copy
@junction:.agent-output/                      # 常時 junction
@tag:deps @junction:node_modules/.pnpm/       # 条件付き junction
@tag:slow some/large/dir/                     # 条件付き copy
```

#### `@junction:` directive (Windows)

`@junction:<pattern>` でcopy代わりにNTFS junctionを作成し、worktreeとmainで同一実体を共有する。大規模untracked dir (`.agent-output/` 等)のcopy時間を回避するための仕組み。

- 例: `@junction:.agent-output/` でcopy 180s → junction 0.1s。
- 副作用: agentの書き込みがmain側に即時反映される(=並列agent間でも見える)。task-id subdirに閉じた書き込み前提で運用する。
- cleanup: `agent-merge-back.sh` の `unjunction_worktree()` が `git worktree remove` 前に junction を `cmd /c rmdir` で剥がす。これを怠ると junction を辿って main 側実体が削除されるので merge-back を bypass しない。
- ディレクトリにのみ適用 (ファイルは無視)。mklink失敗時はcopyにfallback。

#### `@tag:` directive

`@tag:<name>` を付けた行は spawn-prep に `--skip-tag <name>` が渡された時のみ無視される。project 固有の条件付き allowlist (一部タスクで opt-out したいケース) を表現する汎用機構。

- 記法: 1 行 1 タグ。複数 skip は CLI で表現:
  - カンマ区切り: `--skip-tag deps,wasm`
  - 複数回指定: `--skip-tag deps --skip-tag wasm`
- 順序自由: `@tag:deps @junction:foo/` でも `@junction:foo/ @tag:deps` でも同等。
- タグ名は project が定義。汎用 spawn-prep は文字列として扱うのみ。
- 推奨タグ規約 (project が採用する場合):
  - `deps`: package manager の virtual store / cache を junction する行に付ける。Lead は依存変更タスク (package.json / lockfile を編集する可能性のあるタスク) で `--skip-tag deps` を指定し、worktree 内で fresh install させる。これを怠ると worktree 側の依存変更が main 側 store を破壊するリスクあり (e.g. pnpm の prune)。

#### allowlistの自己改善(Lead責務)

agentがworktree内で「ファイルが見つからない/参照できない」とSendMessageで報告した場合、Leadは以下を判断する:

1. **不足ファイルがmain側に存在し、tracked化すべきでない(中間生成物・秘匿情報・local設定等)** → allowlistへ追加
   - そのプロジェクト固有なら `<repo>/.claude/worktree-copy.list` (作成→commitするとチーム共有)
   - 全プロジェクト共通(例: 新たな`.aiconfig`)なら `~/.claude/worktree-copy.list` (dotfilesでcommit)
2. **不足ファイルをtrackedにすべき** → main側で`git add`+commitを指示。allowlist追加は不要
3. **不要な参照(plannerの誤指示等)** → allowlist追加せず、上流memberへ再指示

追加後は既存のworktreeでは反映されないため、必要なら当該worktreeを破棄→agent-spawn-prep.sh再実行。allowlistの追加は1パターン1行で最小限に保ち、`node_modules/`等の重量級は基本入れない(必要なら明示判断)。

### project init hook (worktree-init.sh)

`<repo>/.claude/worktree-init.sh` が存在すれば、agent-spawn-prep.sh が allowlist コピー後に worktree内で実行する。allowlistでコピーするには重すぎる/不適切なものを worktree作成時に整える用途。

典型例:
- `pnpm install --frozen-lockfile --prefer-offline` (node_modules復元。allowlistコピーだと10万ファイル超 + pnpm symlink破綻リスク)
- 大型ビルド成果物の symlink/junction 作成
- `.env` 系の生成・復元

失敗するとspawn中断(worktreeは残る)。member が壊れた環境で動くより明示エラーを優先。
