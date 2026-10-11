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

# Keep the user's and the system's git config (hooks, signing, autocrlf)
# out of the throwaway repos, and name the committer here instead.
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.com
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.com

# new_repo <name> <autocrlf> <committed lines...>: a repo with f.txt committed
new_repo() {
  local dir="$work/$1" autocrlf=$2
  shift 2
  git init -q "$dir"
  git -C "$dir" config core.autocrlf "$autocrlf"
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

# Bytes as hex, for cases where the final newline or the encoding matters.
hex() { od -An -tx1 | tr -d ' \n'; }
staged_hex() { git -C "$1" show :f.txt | hex; }

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
# Git Bash's sed -i writes every line with LF, so unix2dos puts CRLF back.
set_line "$d" 1 A-other && unix2dos -q "$d/f.txt"
(cd "$d" && "$TOOL" begin f.txt && sed -i '3s/.*/C-mine/' f.txt && unix2dos -q f.txt && "$TOOL" stage f.txt >/dev/null)
check "crlf: the working tree is CRLF on every line" "$(printf 'A-other\r\nb\r\nC-mine\r\n' | hex)" "$(hex < "$d/f.txt")"
check "crlf: the blob keeps LF" "0" "$(git -C "$d" show :f.txt | tr -cd '\r' | wc -c | tr -d ' ')"
check "crlf: only mine is staged" "$(printf '%s\n' a b C-mine)" "$(staged "$d")"

# In the refusal cases a failed setup step or begin exits 98 or 99, so only
# a refusal from stage itself gives the 1 they expect.

# 7. My edit on a line someone else changed is refused; the index stays.
d=$(new_repo overlap false a b c)
set_line "$d" 2 B-other
(cd "$d" || exit 98; "$TOOL" begin f.txt || exit 99; sed -i '2s/.*/B-mine/' f.txt || exit 98; "$TOOL" stage f.txt >/dev/null 2>&1)
check "overlap: refused" "1" "$?"
check "overlap: index unchanged" "$(printf '%s\n' a b c)" "$(staged "$d")"

# 8. Stage without begin, and stage with no edit, are refused.
d=$(new_repo misuse false a b)
(cd "$d" || exit 98; sed -i '1s/.*/A-mine/' f.txt || exit 98; "$TOOL" stage f.txt >/dev/null 2>&1)
check "no begin: refused" "1" "$?"
(cd "$d" || exit 98; "$TOOL" begin f.txt || exit 99; "$TOOL" stage f.txt >/dev/null 2>&1)
check "no edit: refused" "1" "$?"

# 9. A file in a subdirectory, named from inside it.
d=$(new_repo subdir false top)
mkdir "$d/sub" && printf '%s\n' a b > "$d/sub/g.txt"
git -C "$d" add sub/g.txt && git -C "$d" commit -qm sub
sed -i '1s/.*/A-other/' "$d/sub/g.txt"
(cd "$d/sub" && "$TOOL" begin g.txt && sed -i '2s/.*/B-mine/' g.txt && "$TOOL" stage g.txt >/dev/null)
check "subdir: only mine is staged" "$(printf '%s\n' a B-mine)" "$(git -C "$d" show :sub/g.txt)"
# begin from the subdirectory and stage from the top find the same copy
# (A-other is still unstaged on line 1).
(cd "$d/sub" && "$TOOL" begin g.txt && sed -i '2s/.*/B-mine2/' g.txt && cd .. && "$TOOL" stage sub/g.txt >/dev/null)
check "subdir then top: only mine is staged" "$(printf '%s\n' a B-mine2)" "$(git -C "$d" show :sub/g.txt)"

# 10. My insertion and my edit of the next line land in order, even with
# someone else's line between them in the working tree.
d=$(new_repo insert-then-edit false a c)
sed -i '1a Y-other' "$d/f.txt"
(cd "$d" && "$TOOL" begin f.txt && printf '%s\n' a X-mine Y-other C-mine > f.txt && "$TOOL" stage f.txt >/dev/null)
check "insert then edit: both land in order" "$(printf '%s\n' a X-mine C-mine)" "$(staged "$d")"

# 11. My insertions on both sides of someone else's line keep their order.
d=$(new_repo two-inserts false a c)
sed -i '1a Y-other' "$d/f.txt"
(cd "$d" && "$TOOL" begin f.txt && printf '%s\n' a X1-mine Y-other X2-mine c > f.txt && "$TOOL" stage f.txt >/dev/null)
check "two insertions: both land in order" "$(printf '%s\n' a X1-mine X2-mine c)" "$(staged "$d")"

# 12. An absolute path, in Windows form (C:/...) and in POSIX form, named
# from a subdirectory; a file in another repository is refused.
for form in windows posix; do
  d=$(new_repo "absolute-$form" false a b c)
  mkdir "$d/sub"
  file="$d/f.txt"
  [ "$form" = windows ] && file=$(cygpath -m "$file" 2>/dev/null || printf '%s' "$file")
  set_line "$d" 1 A-other
  (cd "$d/sub" && "$TOOL" begin "$file" && set_line "$d" 3 C-mine && "$TOOL" stage "$file" >/dev/null)
  check "absolute ($form): only mine is staged" "$(printf '%s\n' a b C-mine)" "$(staged "$d")"
done
o=$(new_repo outside false a)
(cd "$d" || exit 98; "$TOOL" begin "$o/f.txt" >/dev/null 2>&1)
check "outside: refused at begin" "1" "$?"

# 13. Bytes that are not UTF-8 (cp932) pass through unchanged.
d=$(new_repo cp932 false a b c)
printf 'a\n\202\240\nc\n' > "$d/f.txt" && git -C "$d" commit -qam cp932
set_line "$d" 1 A-other
(cd "$d" && "$TOOL" begin f.txt && printf 'A-other\n\202\240\nC-mine\203A\n' > f.txt && "$TOOL" stage f.txt >/dev/null)
check "cp932: bytes pass through" "$(printf 'a\n\202\240\nC-mine\203A\n' | hex)" "$(staged_hex "$d")"

# 14. Someone else's removal of the final newline stays out; my own is staged.
d=$(new_repo eol-theirs false a b)
printf 'a\nb' > "$d/f.txt"
(cd "$d" && "$TOOL" begin f.txt && printf 'A-mine\nb' > f.txt && "$TOOL" stage f.txt >/dev/null)
check "final newline: theirs stays out" "$(printf 'A-mine\nb\n' | hex)" "$(staged_hex "$d")"
d=$(new_repo eol-mine false a b)
set_line "$d" 1 A-other
(cd "$d" && "$TOOL" begin f.txt && printf 'A-other\nb' > f.txt && "$TOOL" stage f.txt >/dev/null)
check "final newline: mine is staged" "$(printf 'a\nb' | hex)" "$(staged_hex "$d")"

# 15. Deleting every line stages an empty blob.
d=$(new_repo delete-all false a b)
(cd "$d" && "$TOOL" begin f.txt && : > f.txt && "$TOOL" stage f.txt >/dev/null)
check "delete all: the blob is empty" "0" "$(git -C "$d" cat-file -s :f.txt)"

# 16. A difflib.py in the repo is not imported in place of the standard one.
d=$(new_repo shadow false a b)
printf 'raise SystemExit("shadowed")\n' > "$d/difflib.py"
set_line "$d" 1 A-other
(cd "$d" && "$TOOL" begin f.txt && sed -i '2s/.*/B-mine/' f.txt && "$TOOL" stage f.txt >/dev/null)
check "shadow: the standard library is used" "$(printf '%s\n' a B-mine)" "$(staged "$d")"

[ "$failures" -eq 0 ] && echo "all passed" || echo "$failures failed"
[ "$failures" -eq 0 ]
