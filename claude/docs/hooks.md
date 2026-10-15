# Claude Code hook(yhooky)

hook は yhooky(github.com/ycookiey/yhooky、Rust の 1 実行ファイル、scoop の yscoopy から導入)で動かす。`.sh` を作って settings.json に登録する形は廃止(Git Bash のプロセスが 1 本 0.5〜1.5 秒、親が終わった後に残る・CPU を回し続ける)。

## 構成

- 登録: テンプレート `claude/settings.json` の `PreToolUse`・`PostToolUse`(matcher `*`)と `Stop` に yhooky を 1 つずつ。exec form(`"command": "yhooky", "args": ["hook"]`)で shell を通さない。`dotcli sync` で全アカウントへ
- 規則の読み込み順: `~/.claude/hooks/rules/*.toml`(= dotfiles `claude/hooks/rules/`)→ `$CLAUDE_PROJECT_DIR/.claude/hooks/rules/*.toml`
- 失敗時: 何も出さず exit 0。理由は `~/.claude/logs/yhooky.log`
- SessionStart(ysidepaney register・claude-pid-session)と project の SessionStart・Read の hook は旧来の bash のまま(起動時 1 回・対象が狭い)

## 規則を足す

1. 正規表現で書けるもの(入力の項目が当たれば文を差し込む / 止める): `common.toml`(全環境)か project の `rules/*.toml` に `[[rule]]` を足す。項目は yhooky の README
   - 照合は 1 行ずつ(grep -E と同じ。`^` `$` は各行)。Rust regex: 後方参照・先読みなし、`[[:space:]]` 等は ASCII
   - 同じ group の規則は最初に当たった 1 つだけ。`{repo_dir}`・`{repo_name}` を文に使える
   - PreToolUse の context は tool の結果と一緒に届く(操作の後)。取り消せない操作(送信・push・消去)への注意は `action = "deny-once"`(session と照合した文ごとに一度だけ止め、出し直せば知らせだけで通す)。後から直せる操作(入力欄への入力・ファイルの編集)は context で足りる
2. 横の `tests/*.toml` に case(quiet / context / deny と、当たる・当たらない両方)を足す
3. 確かめ: `yhooky test <tests/x.toml>`、`yhooky check`(読めない規則の報告)、`echo '<入力 JSON>' | yhooky explain`(各規則の結果)
4. 規則のファイルは保存した時点で全 session に効く(build・再起動は不要)

## 組み込みの判定が要るとき

引用の解析・ファイルや git の確認・状態の記憶・transcript の読み取りは正規表現で書けない → yhooky の `src/builtin/` に足し、`builtins = [...]` で有効にする。release(GitHub)→ yscoopy の manifest は自動更新 → `scoop update yhooky`。一覧は README の「組み込みの判定」

## 新しい出来事(UserPromptSubmit 等)

テンプレートにその出来事の yhooky の登録を足す(exec form)。規則の `event` はその名前

## 罠

- Windows の Claude Code は shell form の hook・statusLine を Git Bash で起動する。MSYS は子を一時停止の状態で作ってから動かすので、その間に bash が止められると子が一時停止のまま残る(推定)。2026-10-11 に jq 594 個・claude-statusline 58 個が親なしで残っていた
- 残ったプロセスは CPU は使わないが、全プロセスのパスを読む処理(`Get-Process | % Path`、scoop の update・uninstall の起動中チェック)が 1 個あたり数秒待たされ、何分も止まる
- 対策: タスク スケジューラの `OrphanReaper`(`bin/reap-orphans.ps1`、`startup/register.ps1` が登録)が 5 分ごとに、親のない Git Bash の道具・jq・claude-statusline のうち、一時停止のまま 30 分以上のもの(git も含む)と、2 回続けて CPU を使い続けたものを止める。ほかの実行ファイルの空回りは止めずに記録する。記録は `~/.local/state/orphan-reaper/log.txt`(「busy, left running」が想定外の漏れの元)。試すときは `-DryRun`
- 道具の一覧にない名前は、空回りしても記録だけで止まらない。2026-10-11 には `find /`(Bash ツールの時間切れの後に残ったもの)3 つが数時間 1 コアずつ使い、記録だけされていた。`find.exe` を一覧に足し、ルートから始める find は yhooky の規則 `find-root-guard` が止める。記録するだけでは誰も読まなかったので、「busy, left running」を記録した回は `dotcli notify` で画面に知らせる(1 回の見回りで 1 つ、60 秒)。知らせが出たら、その名前を一覧に足すか考える
- statusLine には exec form がない(公式 docs は hooks にだけ `args` を書く)
