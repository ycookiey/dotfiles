# config.nu — nushell 設定

$env.config.show_banner = "short"

# --- PATH ---
$env.PATH = ($env.PATH | split row (char esep)
  | append 'C:\Main\Script\git-worktree-runner\bin'
  | append ($env.USERPROFILE | path join 'scoop\apps\miktex\current\texmfs\install\miktex\bin\x64')
  | append ($env.LOCALAPPDATA | path join 'Android\Sdk\platform-tools')
  | uniq)

# --- starship ---
use ($nu.default-config-dir | path join 'cache/starship.nu')

# --- zoxide ---
source ($nu.default-config-dir | path join 'cache/zoxide.nu')

# --- Generated aliases (from dotcli) ---
source ($nu.default-config-dir | path join 'generated-aliases.nu')

# --- yazi (TUI needs direct terminal, can't pipe through dotcli) ---
def y [...args: string] {
    let tmp = (mktemp -t "yazi-cwd.XXXXXX")
    yazi ...$args --cwd-file $tmp
    let cwd = (open $tmp | str trim)
    if $cwd != "" and $cwd != $env.PWD { cd $cwd }
    rm -p $tmp
}
