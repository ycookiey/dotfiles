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

# --- run-task (F5) ---
source ($nu.default-config-dir | path join 'run-task.nu')

$env.config = ($env.config | upsert keybindings (
    ($env.config.keybindings? | default []) | append {
        name: run_task
        modifier: none
        keycode: f5
        mode: [emacs, vi_normal, vi_insert]
        event: { send: executehostcommand, cmd: "run-task" }
    }
))

# --- yazi (TUI needs direct terminal, can't pipe through dotcli) ---
# --- Background sync (settings.json / scoopfile / wingetfile / MCP servers) ---
if (which dotcli | is-not-empty) {
    job spawn { ^dotcli sync --dot $env.DOT | ignore } | ignore
}

# --- yazi (TUI needs direct terminal, can't pipe through dotcli) ---
def y [...args: string] {
    let tmp = (mktemp -t "yazi-cwd.XXXXXX")
    yazi ...$args --cwd-file $tmp
    let cwd = (open $tmp | str trim)
    if $cwd != "" and $cwd != $env.PWD { cd $cwd }
    rm -p $tmp
}

# --- Dotfiles auto-update (background git-prompt) ---
$env.DOTCLI_JOB_ID = null

# --- Build outdated check (background) ---
$env.BUILD_CHECK_JOB_ID = null

$env.config = ($env.config | upsert hooks.pre_prompt {|cfg|
    let existing = try { $cfg.hooks.pre_prompt } catch { [] }
    $existing | append { ||
        # ① 前回 job の結果を回収して表示（1回のみ）
        if ($env.DOTCLI_JOB_ID != null and $env.DOTCLI_JOB_ID != -1) {
            let msg = try { job recv --timeout 0sec } catch { null }
            if ($msg != null) {
                let trimmed = ($msg | str trim)
                if ($trimmed | is-not-empty) {
                    print $trimmed
                }
                $env.DOTCLI_JOB_ID = -1
            } else {
                # job が既に終了していたら完了扱い
                let running = (job list | where id == $env.DOTCLI_JOB_ID | length)
                if $running == 0 { $env.DOTCLI_JOB_ID = -1 }
            }
        }
        # ② 初回のみ job spawn（dotcli が PATH にある場合のみ）
        if ($env.DOTCLI_JOB_ID == null and (which dotcli | is-not-empty)) {
            let dot = $env.DOT
            $env.DOTCLI_JOB_ID = (job spawn {
                let r = (^dotcli git-prompt $dot | complete)
                if $r.exit_code == 0 { $r.stdout | job send 0 }
            })
        }
        # ③ build --check の結果を回収して表示（1回のみ）
        if ($env.BUILD_CHECK_JOB_ID != null and $env.BUILD_CHECK_JOB_ID != -1) {
            let msg = try { job recv --tag 1 --timeout 0sec } catch { null }
            if ($msg != null) {
                let trimmed = ($msg | str trim)
                if ($trimmed | is-not-empty) {
                    print $trimmed
                }
                $env.BUILD_CHECK_JOB_ID = -1
            } else {
                let running = (job list | where id == $env.BUILD_CHECK_JOB_ID | length)
                if $running == 0 { $env.BUILD_CHECK_JOB_ID = -1 }
            }
        }
        # ④ 初回のみ build --check を job spawn
        if ($env.BUILD_CHECK_JOB_ID == null and (which dotcli | is-not-empty)) {
            $env.BUILD_CHECK_JOB_ID = (job spawn {
                let r = (^dotcli build --check | complete)
                if $r.exit_code != 0 {
                    let out = ($r.stdout + $r.stderr)
                    if ($out | str trim | is-not-empty) { $out | job send 1 }
                }
            })
        }
    }
})
