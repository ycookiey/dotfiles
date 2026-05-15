---
name: urleader
description: Agent teamのleadとして振る舞うモードを起動する。
allowed-tools: Read, Bash, TeamCreate, Agent, SendMessage, TaskCreate, TaskList, TaskUpdate, TaskGet
---

`~/.claude/docs/agent-team.md` を読み込み、そこに記載された原則に従ってleadとして動作を開始する。読み込み後、自分がleadであることを宣言し、利用可能なmember一覧を示してユーザーにタスクを促す。

## Worktree Isolation

implementer/tester等のsubagent spawn時は、以下の手順でworktree分離を行う。

### spawn手順

1. `agent-spawn-prep.sh`を実行して worktree作成 + guard config書き出し:
   ```bash
   bash "<skill-root>/scripts/agent-spawn-prep.sh" --task-id "<TASK_ID>"
   ```
   依存変更タスク(package.json/lockfile を編集する可能性あり)では `--skip-tag deps` を追加:
   ```bash
   bash "<skill-root>/scripts/agent-spawn-prep.sh" --task-id "<TASK_ID>" --skip-tag deps
   ```
   詳細は agent-team.md の `@tag:` directive セクション参照。
2. stdout出力から`WORKTREE_ROOT`を取得
3. **失敗時(exit非0)**: stderrメッセージを確認。branch名衝突ならtask-idを変更(`<TASK_ID>-2`等)して再試行。その他のエラーはstderrの内容から判断
4. prompt 文字列を組み立てる:
   - 絶対パスが必要な箇所は `<WORKTREE_ROOT>` を直接埋め込む (main repo path をそのまま書かない — worktree 外参照になる)
   - 相対パスでよい箇所は cwd 基点 (= `<WORKTREE_ROOT>` 配下) で書く
5. **(任意) prompt 永続化**: 監査・再現用に `<WORKTREE_ROOT>/.agent-output/<task-id>/spawned-prompt.md` 等へ Write tool で保存。spawn-prep の入力ではないため Bash 呼び出しと並列発火可
6. Agent toolでsubagentをspawn。promptには:
   - 組み立て済みprompt文字列
   - `export WORKTREE_GUARD_ROOT="<WORKTREE_ROOT>"` の指示(補助。hookはconfigファイルから自動解決するが、明示性のため残す)
   - 「worktree内(`<WORKTREE_ROOT>`)でのみ作業すること」の明示
   - 「`.agent-output/` への書き込みはEdit/Write toolを使用すること(Bash redirect不可)」の明示

### researcher/plannerの例外

読み取り専用のresearcher/plannerにはworktree分離は不要。書き込みを行うsubagent(implementer, tester, reviewer(コード修正時))にのみ適用する。

### merge-back & cleanup

subagentがcommit済みの変更をmainへ取り込み、worktree/branchを削除する。`agent-merge-back.sh`がrebase→ff-only merge→cleanupを一括で行う:

```bash
bash "<skill-root>/scripts/agent-merge-back.sh" --task-id "<TASK_ID>"
```

挙動:
- worktree内で`git rebase main`しdiverge解消(並列agent運用で必発)
- main側で`git merge --ff-only`、成功時のみ`worktree remove`+`branch -D`
- rebase conflict時は`rebase --abort`+非0 exitでLeadへ返す(worktree保持)
- pushしない

merge失敗時はworktree/branchが残るので、Leadはconflict解消を上流memberに再指示するか手動で対処する。
