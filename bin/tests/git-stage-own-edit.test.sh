#!/usr/bin/env bash
# Cases for git-stage-own-edit: run `bash bin/tests/git-stage-own-edit.test.sh`.
# Each case makes a throwaway repo, lets "someone else" change the file
# without staging, then makes "your" edit between begin and stage, and
# checks what the index holds. Prints OK/NG per case and exits 1 when
# any case fails.

set -uo pipefail

TOOL="$(cd "$(dirname "$0")/.." && pwd)/git-stage-own-edit"
failures=0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# new_repo <name> <autocrlf> <committed lines...>: a repo with f.txt committed
new_repo() {
  local dir="$work/$1" autocrlf=$2
  shift 2
  git init -q "$dir"
  git -C "$dir" config core.autocrlf "$autocrlf"
  git -C "$dir" config user.email test@example.com
  git -C "$dir" config user.name test
  printf '%s\n' "$@" > "$dir/f.txt"
  git -C "$dir" add f.txt
  git -C "$dir" commit -qm init
  echo "$dir"
}

# check <name> <want> <got>
check() {
  if [ "$2" = "$3" ]; then
    printf 'OK %s\n' "$1"
  else
    printf 'NG %s\n  want: %s\n  got:  %s\n' "$1" "$(printf '%s' "$2" | tr '\n' '|')" "$(printf '%s' "$3" | tr '\n' '|')"
    failures=$((failures + 1))
  fi
}

staged() { git -C "$1" show :f.txt | tr -d '\r'; }

# Replace line <n> of f.txt with <text>.
set_line() { sed -i "$2s/.*/$3/" "$1/f.txt"; }

# 1. Someone else's change far away stays out.
d=$(new_repo far false a b c d e f g h)
set_line "$d" 1 A-other
(cd "$d" && "$TOOL" begin f.txt && sed -i '8s/.*/H-mine/' f.txt && "$TOOL" stage f.txt >/dev/null)
check "far: only mine is staged" "$(printf '%s\n' a b c d e f g H-mine)" "$(staged "$d")"
check "far: theirs stays in the working tree" "A-other" "$(head -1 "$d/f.txt")"

# 2. Someone else's change on the line next to mine stays out.
d=$(new_repo adjacent false a b c d)
set_line "$d" 2 B-other
(cd "$d" && "$TOOL" begin f.txt && sed -i '3s/.*/C-mine/' f.txt && "$TOOL" stage f.txt >/dev/null)
check "adjacent: only mine is staged" "$(printf '%s\n' a b C-mine d)" "$(staged "$d")"

# 3. Lines someone else added above move mine; it still lands on its line.
d=$(new_repo shifted false a b c d)
sed -i '1i x1\nx2' "$d/f.txt"
(cd "$d" && "$TOOL" begin f.txt && sed -i '5s/.*/c-mine/' f.txt && "$TOOL" stage f.txt >/dev/null)
check "shifted: mine lands on its own line" "$(printf '%s\n' a b c-mine d)" "$(staged "$d")"

# 4. My insertion after a line someone else changed goes after that line.
d=$(new_repo insert false a b c)
set_line "$d" 2 B-other
(cd "$d" && "$TOOL" begin f.txt && sed -i '1a new-mine' f.txt && "$TOOL" stage f.txt >/dev/null)
check "insert: mine goes in its place" "$(printf '%s\n' a new-mine b c)" "$(staged "$d")"

# 5. Deleting a line.
d=$(new_repo delete false a b c)
set_line "$d" 3 C-other
(cd "$d" && "$TOOL" begin f.txt && sed -i '1d' f.txt && "$TOOL" stage f.txt >/dev/null)
check "delete: only my deletion is staged" "$(printf '%s\n' b c)" "$(staged "$d")"

# 6. A CRLF working tree over LF blobs (core.autocrlf=true) stages LF.
d=$(new_repo crlf true a b c)
rm "$d/f.txt" && git -C "$d" checkout -q -- f.txt
set_line "$d" 1 'A-other\r'
(cd "$d" && "$TOOL" begin f.txt && sed -i '3s/.*/C-mine\r/' f.txt && "$TOOL" stage f.txt >/dev/null)
check "crlf: the blob keeps LF" "0" "$(git -C "$d" show :f.txt | tr -cd '\r' | wc -c | tr -d ' ')"
check "crlf: only mine is staged" "$(printf '%s\n' a b C-mine)" "$(staged "$d")"

# 7. My edit on a line someone else changed is refused; the index stays.
d=$(new_repo overlap false a b c)
set_line "$d" 2 B-other
(cd "$d" && "$TOOL" begin f.txt && sed -i '2s/.*/B-mine/' f.txt && "$TOOL" stage f.txt >/dev/null 2>&1)
check "overlap: refused" "1" "$?"
check "overlap: index unchanged" "$(printf '%s\n' a b c)" "$(staged "$d")"

# 8. Stage without begin, and stage with no edit, are refused.
d=$(new_repo misuse false a b)
(cd "$d" && sed -i '1s/.*/A-mine/' f.txt && "$TOOL" stage f.txt >/dev/null 2>&1)
check "no begin: refused" "1" "$?"
(cd "$d" && "$TOOL" begin f.txt && "$TOOL" stage f.txt >/dev/null 2>&1)
check "no edit: refused" "1" "$?"

# 9. A file in a subdirectory, named from inside it.
d=$(new_repo subdir false top)
mkdir "$d/sub" && printf '%s\n' a b > "$d/sub/g.txt"
git -C "$d" add sub/g.txt && git -C "$d" commit -qm sub
sed -i '1s/.*/A-other/' "$d/sub/g.txt"
(cd "$d/sub" && "$TOOL" begin g.txt && sed -i '2s/.*/B-mine/' g.txt && "$TOOL" stage g.txt >/dev/null)
check "subdir: only mine is staged" "$(printf '%s\n' a B-mine)" "$(git -C "$d" show :sub/g.txt)"

[ "$failures" -eq 0 ] && echo "all passed" || echo "$failures failed"
[ "$failures" -eq 0 ]
