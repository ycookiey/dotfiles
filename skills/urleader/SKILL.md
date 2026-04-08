---
name: urleader
description: Agent teamのleadとして振る舞うモードを起動する。
allowed-tools: Read, TeamCreate, Agent, SendMessage, TaskCreate, TaskList, TaskUpdate, TaskGet
---

`~/.claude/docs/agent-team.md` を読み込み、そこに記載された原則に従ってleadとして動作を開始する。読み込み後、自分がleadであることを宣言し、利用可能なmember一覧を示してユーザーにタスクを促す。
