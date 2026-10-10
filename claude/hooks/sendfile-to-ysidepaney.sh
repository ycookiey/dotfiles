#!/usr/bin/env bash
# sendfile-to-ysidepaney.sh — PreToolUse hook (SendUserFile):
# 画像はユーザーが会話の横に開いている ysidepaney で見るので、SendUserFile で送ろうとした画像
# (png・jpg・jpeg・gif・webp・bmp)を `ysidepaney post image` で置き、SendUserFile は止める。
# 画像以外も含むときは、画像だけを置いて止め、画像以外を送り直すよう返す。
# 画像が無いとき、ysidepaney が無いとき、置けなかったときは何もせず通す。
# ysidepaney の skill があっても、画像を SendUserFile だけで送り、ユーザーに言われるまで
# 置かなかった(2026-10-10)。
# YSIDEPANEY_BIN で置くコマンドを差し替えられる(test 用)。
input=$(cat)
bin=${YSIDEPANEY_BIN:-ysidepaney}
command -v "$bin" >/dev/null 2>&1 || exit 0

# Windows の jq は改行に \r を付けるので、読むたびに外す。
field() { jq -r "$1" <<<"$input" | tr -d '\r'; }
cwd=$(field '.cwd // ""')
session=$(field '.session_id // ""')
caption=$(field '.tool_input.caption // ""' | head -n 1 | cut -c1-80)

images=()
others=()
while IFS= read -r file; do
  [ -n "$file" ] || continue
  case "${file,,}" in
    *.png | *.jpg | *.jpeg | *.gif | *.webp | *.bmp) images+=("$file") ;;
    *) others+=("$file") ;;
  esac
done < <(field '.tool_input.files // [] | .[]')
[ "${#images[@]}" -gt 0 ] || exit 0

posted=()
for file in "${images[@]}"; do
  path=$file
  # 相対パスは会話の作業フォルダから。
  case "$path" in
    /* | [A-Za-z]:[\\/]*) ;;
    *) path="$cwd/$path" ;;
  esac
  name=${path##*[\\/]}
  title=${caption:-$name}
  [ -n "$caption" ] && [ "${#images[@]}" -gt 1 ] && title="$caption ($name)"
  args=(post image "$path" --title "$title")
  [ -n "$session" ] && args+=(--session "$session")
  "$bin" "${args[@]}" >/dev/null 2>&1 || exit 0
  posted+=("$name")
done

reason="画像 ${#posted[@]} 枚(${posted[*]})を ysidepaney に置いた。画像は SendUserFile で送らない。会話では、何を確かめる画像かを文で伝える。"
if [ "${#others[@]}" -gt 0 ]; then
  reason="$reason 画像以外(${others[*]})を送るなら、それだけで SendUserFile をもう一度呼ぶ。"
fi
jq -nc --arg r "$reason" \
  '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: $r}}'
