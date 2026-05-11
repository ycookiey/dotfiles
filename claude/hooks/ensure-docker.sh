#!/bin/bash
# PreToolUse hook (Bash matcher): docker系コマンド検知時にDocker Desktop未起動なら起動し疎通まで待機。
# 未インストール環境ではno-op。タイムアウト時はexit 2でブロック。

set -u

DOCKER_DESKTOP_EXE="/c/Program Files/Docker/Docker/Docker Desktop.exe"
# WSL2コールドスタート + Engine初期化で2分超えることがある
WAIT_SECONDS=180
POLL_INTERVAL=2

input=$(cat)
command=$(printf '%s' "$input" | jq -r '.tool_input.command // empty')

[ -z "$command" ] && exit 0

# パイプ/論理演算子の後も含め、tokenとしてのdocker/docker-composeを検知
if ! printf '%s' "$command" | grep -Eq '(^|[|;&`]|&&|\|\|)[[:space:]]*(sudo[[:space:]]+)?docker(-compose)?([[:space:]]|$)'; then
  exit 0
fi

# Server応答までチェック。docker info は Server部500でも exit 0 を返すため不可
is_ready() {
  docker version --format '{{.Server.Version}}' >/dev/null 2>&1
}

if is_ready; then
  exit 0
fi

if [ ! -x "$DOCKER_DESKTOP_EXE" ]; then
  exit 0
fi

# Windowsの`start`経由で完全に切り離して起動。`bash &`+`disown` だと
# hook(bash)プロセスツリー終了時にGUIアプリが巻き添えで死ぬことがある
cmd.exe //c start "" "$(cygpath -w "$DOCKER_DESKTOP_EXE")" >/dev/null 2>&1

elapsed=0
while [ "$elapsed" -lt "$WAIT_SECONDS" ]; do
  if is_ready; then
    printf 'Docker Desktop started (%ds).\n' "$elapsed" >&2
    exit 0
  fi
  sleep "$POLL_INTERVAL"
  elapsed=$((elapsed + POLL_INTERVAL))
done

jq -n --arg sec "$WAIT_SECONDS" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "block",
    reason: ("Docker Desktop did not become ready within " + $sec + "s. Linux Engine may be hung. Recovery: 1) Quit Docker Desktop from tray, 2) Run `wsl --shutdown` in PowerShell, 3) Start Docker Desktop again.")
  }
}'
exit 0
