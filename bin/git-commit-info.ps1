#!/usr/bin/env pwsh
git status --short
git diff -U1 --no-prefix --no-color | ? { $_ -notmatch '^index ' }
git diff --cached -U1 --no-prefix --no-color | ? { $_ -notmatch '^index ' }
git log --oneline -5

# Safety check: scan added lines only
$keywords = @('TODO', 'FIXME', 'console\.log', 'debugger',
    'api_key', 'secret', 'token', 'password', 'credential',
    'sk-', 'ghp_', 'AKIA', 'ycook')
$diff = git diff HEAD -U0 --no-color 2>$null
if ($diff) {
    foreach ($line in $diff) {
        if ($line.StartsWith('+') -and !$line.StartsWith('+++')) {
            foreach ($kw in $keywords) {
                if ($line -match $kw) {
                    $clean = $line.Substring(1).Trim()
                    echo "WARNING: Found '$kw' -> $clean"
                }
            }
        }
    }
}
