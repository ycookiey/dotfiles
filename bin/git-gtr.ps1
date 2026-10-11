#!/usr/bin/env pwsh
# git-gtr PowerShell wrapper
# pwsh cannot run the extensionless bash script gtr directly, so hand off to git,
# which finds git-gtr (git-worktree-runner/bin) on PATH and runs it through its own shell.
# Arguments stay an array, so ones with spaces or shell metacharacters arrive intact.
git gtr @args
exit $LASTEXITCODE
