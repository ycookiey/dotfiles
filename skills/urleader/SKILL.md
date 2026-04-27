---
name: urleader
description: Agent teamのleadとして振る舞うモードを起動する。
allowed-tools: Read, Bash, TeamCreate, Agent, SendMessage, TaskCreate, TaskList, TaskUpdate, TaskGet
---

`~/.claude/docs/agent-team.md` を読み込み、そこに記載された原則に従ってleadとして動作を開始する。読み込み後、自分がleadであることを宣言し、利用可能なmember一覧を示してユーザーにタスクを促す。

## Worktree Isolation

implementer/tester等のsubagent spawn時は、以下の手順でworktree分離を行う。

### spawn手順

1. prompt内容をファイルに書き出す (`/tmp/agent-prompt-<task-id>.md` 等)
2. `agent-spawn-prep.sh`を実行して worktree作成 + prompt内パス置換 + guard config書き出し:
   ```bash
   bash "<skill-root>/scripts/agent-spawn-prep.sh" \
     --task-id "<TASK_ID>" \
     --prompt-file "/tmp/agent-prompt-<task-id>.md"
   ```
3. stdout出力から`WORKTREE_ROOT`を取得
4. **失敗時(exit非0)**: stderrメッセージを確認。branch名衝突ならtask-idを変更(`<TASK_ID>-2`等)して再試行。その他のエラーはstderrの内容から判断
5. Agent toolでsubagentをspawn。promptには:
   - 書き換え済みprompt fileの内容
   - `export WORKTREE_GUARD_ROOT="<WORKTREE_ROOT>"` の指示(補助。hookはconfigファイルから自動解決するが、明示性のため残す)
   - 「worktree内(`<WORKTREE_ROOT>`)でのみ作業すること」の明示
   - 「`.agent-output/` への書き込みはEdit/Write toolを使用すること(Bash redirect不可)」の明示

### researcher/plannerの例外

読み取り専用のresearcher/plannerにはworktree分離は不要。書き込みを行うsubagent(implementer, tester, reviewer(コード修正時))にのみ適用する。

### cleanup

subagent完了後、変更のcommit/pushが済んだworktreeは以下で削除:
```bash
git worktree remove ".claude/worktrees/agent-<TASK_ID>" --force
git branch -D "worktree-agent-<TASK_ID>"
```
