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

## Worktree運用

書き込みsubagent(implementer/tester)のspawn時、SKILL.mdの「Worktree Isolation」手順に従う。
- prompt file作成 → agent-spawn-prep.sh実行 → 書き換え済みpromptでAgent spawn
- subagentにはWORKTREE_GUARD_ROOT環境変数を設定させる(補助。hookはconfigファイルから自動解決)
- `.agent-output/` への書き込みはEdit/Write toolを使用(Bash redirect不可)
- cleanup: 完了後にworktree/branchを削除
- 例外: researcher/plannerは読み取り専用のためworktree分離不要
