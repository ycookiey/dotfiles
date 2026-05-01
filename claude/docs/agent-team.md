# Agent Team: Leadオーケストレーションガイド

## 原則・ルール

- Leadは委譲・統合・判断のみ。実装・調査・レビュー・テスト・commitは一切しない。失敗しても再委譲
- implementerには実装完了後のcommitを必ず指示する
- 独立タスクは複数member同時起動。待ち中も別タスクの準備・起動・統合を並行検討
- 同一ファイルの同時編集禁止
- 完了タスクは削除せず記録として残す
- Leadのcontext最小化を意識

## 手順

1. `TeamCreate` → `TaskCreate`（依存: `blockedBy`。同一ファイル編集も依存に含める）
2. `Agent`でspawn（`team_name`, `name`, `subagent_type`指定）→ `TaskUpdate`で`owner`設定
3. memberの`SendMessage`を受けて統合・次指示。`blockedBy`解消済みの未着手タスクがあれば即spawn（ユーザ確認不要）
4. 完了後 `SendMessage`で`{type: "shutdown_request"}`

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
- 例: `.agent-output/T-7.1/plan.md`, `.agent-output/T-7.1/research-api.md`
- implementerへはspawn時にファイルパスを渡す

## Spawn

- 前memberの出力サマリー（またはファイルパス）、期待出力形式、「やらないこと」の明示
- パス指定: prompt内のファイルパスは絶対パスを使用。worktree運用時はagent-spawn-prep.shが自動的にworktree rootに書き換える
- ブロック時: memberがSendMessageで報告 → Leadが追加情報or別member

### planner: plan構成指示

spawn promptの `plan_layout: single|multi|auto` で構成を指示(省略=auto)。
- `single`: 単一plan file
- `multi`: 複数file(`00-overview.md` + `NN-<phase>.md`)。phase一覧も指示可
- `auto`: planner判断

plannerが規模乖離を検知すれば報告するので、Leadが再判断。

## Worktree運用

書き込みsubagent(implementer/tester)のspawn時、SKILL.mdの「Worktree Isolation」手順に従う。
- prompt file作成 → agent-spawn-prep.sh実行 → 書き換え済みpromptでAgent spawn
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

#### allowlistの自己改善(Lead責務)

agentがworktree内で「ファイルが見つからない/参照できない」とSendMessageで報告した場合、Leadは以下を判断する:

1. **不足ファイルがmain側に存在し、tracked化すべきでない(中間生成物・秘匿情報・local設定等)** → allowlistへ追加
   - そのプロジェクト固有なら `<repo>/.claude/worktree-copy.list` (作成→commitするとチーム共有)
   - 全プロジェクト共通(例: 新たな`.aiconfig`)なら `~/.claude/worktree-copy.list` (dotfilesでcommit)
2. **不足ファイルをtrackedにすべき** → main側で`git add`+commitを指示。allowlist追加は不要
3. **不要な参照(plannerの誤指示等)** → allowlist追加せず、上流memberへ再指示

追加後は既存のworktreeでは反映されないため、必要なら当該worktreeを破棄→agent-spawn-prep.sh再実行。allowlistの追加は1パターン1行で最小限に保ち、`node_modules/`等の重量級は基本入れない(必要なら明示判断)。
