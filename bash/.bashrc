# Git Worktree Runner PATH
export PATH="$PATH:/c/Main/Script/git-worktree-runner/bin"

# Claude Code
export CLAUDE_CODE_MAX_OUTPUT_TOKENS=16384
export PATH="/c/Main/Tool/git-worktree-runner/bin:$PATH"

# Windows PATHEXT emulation for Git Bash
# Bash ignores PATHEXT, so .cmd/.bat files can't be invoked without extension.
# This handler runs only on command-not-found (zero impact on normal execution).
command_not_found_handle() {
  local cmd="$1"; shift
  local ext
  for ext in .cmd .bat; do
    if type "${cmd}${ext}" &>/dev/null; then
      "${cmd}${ext}" "$@"
      return $?
    fi
  done
  printf "bash: %s: command not found\n" "$cmd" >&2
  return 127
}
