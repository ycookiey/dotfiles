#!/usr/bin/env bash
# Cases for language-guard.sh: run `bash claude/hooks/tests/language-guard.test.sh`.
# Prints OK/NG per case and exits 1 when any case fails.

set -uo pipefail
export LC_ALL=C.UTF-8

GUARD="$(cd "$(dirname "$0")/.." && pwd)/language-guard.sh"
failures=0

# expect <block|pass> <stop_hook_active> <message>
expect() {
  local want=$1 active=$2 message=$3 out got
  out=$(jq -nc --arg m "$message" --argjson a "$active" \
    '{last_assistant_message: $m, stop_hook_active: $a}' | bash "$GUARD")
  if [ -n "$out" ]; then got=block; else got=pass; fi
  if [ "$got" = "$want" ]; then
    printf 'OK %-5s %s\n' "$got" "$(printf '%s' "$message" | head -c 60 | tr '\n' ' ')"
  else
    printf 'NG %-5s (want %s) %s\n' "$got" "$want" "$(printf '%s' "$message" | head -c 60 | tr '\n' ' ')"
    failures=$((failures + 1))
  fi
}

expect block false "Found the cause: the kit's ImageUpload shows the saved URL as is, so the icon fails on the http dev server."
expect pass false "原因が分かった。kit の ImageUpload が保存した URL をそのまま表示するので、http の開発サーバーでアイコンが出ない。"
expect pass false "$(printf '確認した。\n\n```ts\nexport const imageSrcOf = (url: string | null | undefined): string | null => {\n  return storedImagePathOf(url) ?? url\n}\n```\n\n`createEntryEditClient` と `useHistoryStepNavigation` を使う。')"
expect pass false "詳しくは https://github.com/example/claude-code-guard/blob/main/hooks/guard.py と https://code.claude.com/docs/en/hooks を参照。"
expect pass false "OK, done."
expect pass true "Found the cause: the kit's ImageUpload shows the saved URL as is, so the icon fails on the http dev server."
expect block false "$(printf '| | Approach | Pros |\n|---|---|---|\n| **U1** | Read the script and vendor it | Covers more than T1 |\n| U2 | Write my own bash | Matches label-guard |')"
expect pass false 'kit の `TournamentEntryEdit` に `renderNoToken`・`renderNotFoundExtras`・`renderDoneActions` の枠を付け、`EntryEditConnection` を受ける。test は 15 件。'
expect pass false "$(printf '> Quoted English text from a peer session that is long enough to count as English.\n\n引用は除いて数える。')"
expect pass false "連絡は support@example.com と [the documentation page for hooks](https://code.claude.com/docs/en/hooks) で受け付ける。"
# Japanese full of identifiers outside backticks: about 11 Latin letters per
# Japanese character, like the densest Japanese replies measured.
expect pass false "Group C-L 全反映完了 fast-forward push。C-3 CharacterPicker fieldset legend count live region を追加、C-4 ImageUpload peer-focus-visible label status live を追加、C-6 Completion Confirmation heading mount focus を移動。"
# English with a few Japanese words quoted in it.
expect block false "The label 「アイコン画像」 is lighter and smaller than the other fields' labels, because ImageUpload uses font-medium while FieldShell uses font-semibold, so I aligned them through a shared constant in a new internal module."

# The transcript's final text after the last tool call is judged too, even
# when last_assistant_message is empty.
transcript=$(mktemp)
cat > "$transcript" <<'JSONL'
{"type":"user","message":{"content":"次の作業へ"}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Checking the files first, then the tests."}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Step two is committed. Next I will write the admin login screen and its list."}]}}
JSONL
out=$(jq -nc --arg t "$transcript" '{transcript_path: $t, stop_hook_active: false}' |
  LANGUAGE_GUARD_WAIT=0 bash "$GUARD")
if printf '%s' "$out" | grep -q 'Step two is committed'; then
  echo "OK block transcript final text"
else
  echo "NG transcript final text: $out"
  failures=$((failures + 1))
fi
if printf '%s' "$out" | grep -q 'Checking the files first'; then
  echo "NG a text before the last tool call was judged"
  failures=$((failures + 1))
else
  echo "OK text before the last tool call is not judged"
fi
check=$(bash "$GUARD" --check "$transcript")
if [ "$check" = "turn 2: Step two is committed. Next I will write the admin login screen and its list." ]; then
  echo "OK --check"
else
  echo "NG --check: $check"
  failures=$((failures + 1))
fi
rm -f -- "$transcript"

[ "$failures" -eq 0 ]
