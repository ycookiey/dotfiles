#!/bin/bash
# Windows パス (C:\...) を Unix パス (/c/...) に正規化。Unix パスはそのまま。
# Usage: normalize-path.sh "C:\foo\bar"  → /c/foo/bar
p="$1"
if [[ "$p" =~ ^[A-Za-z]:\\ ]]; then
  cygpath -u "$p"
else
  printf '%s' "$p"
fi
