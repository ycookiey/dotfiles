# Read this file once per process. A login shell with BASH_ENV set reaches it twice
# (via ~/.bash_profile and via BASH_ENV). The guard is not exported, so child shells
# still read the file. To re-source it by hand, run `unset _DOT_BASHRC_DONE` first.
[ -n "${_DOT_BASHRC_DONE-}" ] && return
_DOT_BASHRC_DONE=1

# Put a directory at the front of PATH using builtins only (no fork).
# Missing directories and entries containing ':' are skipped. An entry already in
# PATH is moved to the front instead of added again, so nested shells keep PATH flat.
_path_prepend() {
  [ -d "$1" ] || return 0
  case "$1" in *:*) return 0 ;; esac
  local p=":$PATH:"
  while [[ $p == *":$1:"* ]]; do p=${p//":$1:"/:}; done
  p=${p#:}; p=${p%:}
  PATH="$1${p:+:$p}"
}

# Claude Code
export CLAUDE_CODE_MAX_OUTPUT_TOKENS=16384

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

# dotcli-generated aliases (mirrors pwsh/nushell)
# readlink -f resolves the ~/.bashrc symlink to an absolute path ('/'-separated even for a
# C:\ path in BASH_ENV), so stripping the last component gives this file's directory.
_dot_bashrc_path=$(readlink -f -- "${BASH_SOURCE[0]}")
_dot_bash_dir=${_dot_bashrc_path%/*}
[ -f "$_dot_bash_dir/generated-aliases.sh" ] && . "$_dot_bash_dir/generated-aliases.sh"

# git-worktree-runner (gtr): the checkout next to the dotfiles repo, derived like $gtrBin in
# pwsh/profile.ps1 and the PATH in nushell/config.nu. This file's directory is <repo>/bash,
# so dropping two components gives the repo's parent (no extra fork). readlink -f keeps the
# drive form (C:/...) for a C:\ path that is not a symlink, and PATH needs /c/...
_dot_parent=${_dot_bash_dir%/*/*}
case $_dot_parent in
  [A-Za-z]:/*) _dot_drive=${_dot_parent:0:1}; _dot_parent="/${_dot_drive,,}${_dot_parent:2}" ;;
esac
[ -n "$_dot_bashrc_path" ] && _path_prepend "$_dot_parent/git-worktree-runner/bin"
unset _dot_bashrc_path _dot_bash_dir _dot_parent _dot_drive

# opencode
_path_prepend "$HOME/.opencode/bin"

export PATH
unset -f _path_prepend
