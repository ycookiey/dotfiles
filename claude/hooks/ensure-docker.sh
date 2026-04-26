#!/bin/bash
# PreToolUse hook (Bash matcher): docker系コマンド検知時にDocker Desktop未起動なら起動し疎通まで待機。
# 未インストール環境ではno-op。タイムアウト時はexit 2でブロック。

set -u

DOCKER_DESKTOP_EXE="/c/Program Files/Docker/Docker/Docker Desktop.exe"
WAIT_SECONDS=60
POLL_INTERVAL=2

input=$(cat)
command=$(printf '%s' "$input" | jq -r '.tool_input.command // empty')

[ -z "$command" ] && exit 0

# パイプ/論理演算子の後も含め、tokenとしてのdocker/docker-composeを検知
if ! printf '%s' "$command" | grep -Eq '(^|[|;&`]|&&|\|\|)[[:space:]]*(sudo[[:space:]]+)?docker(-compose)?([[:space:]]|$)'; then
  exit 0
fi

if docker info >/dev/null 2>&1; then
  exit 0
fi

if [ ! -x "$DOCKER_DESKTOP_EXE" ]; then
  exit 0
fi

# background起動（Git Bashから直接実行）
"$DOCKER_DESKTOP_EXE" >/dev/null 2>&1 &
disown 2>/dev/null || true

elapsed=0
while [ "$elapsed" -lt "$WAIT_SECONDS" ]; do
  if docker info >/dev/null 2>&1; then
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
    reason: ("Docker Desktop did not become ready within " + $sec + "s. Wait and retry, or start it manually.")
  }
}'
exit 0
