# config.nu — nushell 設定
# user コマンド追加は cli/definitions.toml (dotcli generate)。ここは nushell 固有(env/hooks/直操作 def)。

$env.config.show_banner = "short"

# 端末 (WezTerm) に今いるフォルダを OSC 7 で知らせる。Windows では既定で無効。
# 無効だと端末はプロセスの作業フォルダを見るが、nushell は cd してもそれをすぐには変えない
# (`nu -e "cd X"` の直後は起動時のフォルダのまま見えた)
$env.config.shell_integration.osc7 = true

# --- PATH ---
$env.PATH = ($env.PATH | split row (char esep)
  | append 'C:\Main\Script\git-worktree-runner\bin'
  | append ($env.USERPROFILE | path join 'scoop\apps\miktex\current\texmfs\install\miktex\bin\x64')
  | append ($env.LOCALAPPDATA | path join 'Android\Sdk\platform-tools')
  | append ($env.USERPROFILE | path join '.opencode/bin')
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

# --- Background sync (settings.json / scoopfile / wingetfile / MCP servers) ---
# dotcli の有無は起動時に 1 回だけ判定し、pre_prompt hook でも再利用する
$env.DOTCLI_AVAILABLE = (which dotcli | is-not-empty)
if $env.DOTCLI_AVAILABLE {
    job spawn { ^dotcli sync --dot $env.DOT | ignore } | ignore
}

# --- Background starship/zoxide cache refresh (stale のときだけ spawn) ---
# 先頭行の key を inline で比較し、fresh なら nu 子プロセスを起動しない。
# $nu.current-exe は scoop shim を経由しない実体の nu.exe
use ($nu.default-config-dir | path join 'cache-key.nu') cache-stale
if (cache-stale 'starship' ($nu.default-config-dir | path join 'cache/starship.nu')) or (cache-stale 'zoxide' ($nu.default-config-dir | path join 'cache/zoxide.nu')) {
    job spawn {
        ^$nu.current-exe -n ($nu.default-config-dir | path join 'generate-cache.nu') | ignore
    } | ignore
}

# --- yazi (TUI needs direct terminal, can't pipe through dotcli) ---
def --env y [...args: string] {
    let tmp = (mktemp -t "yazi-cwd.XXXXXX")
    yazi ...$args --cwd-file $tmp
    let cwd = (open $tmp | str trim)
    if $cwd != "" and $cwd != $env.PWD { cd $cwd }
    rm -p $tmp
}

# --- chrome-yatcy: CDP 有効 + 専用プロファイルで Chrome 起動 ---
# pwsh の bin/chrome-yatcy.ps1 と等価。詳細はそちらのコメント参照。
def chrome-yatcy [
    --port (-p): int = 9222
    --profile (-P): string = "yatcy"
    ...extra: string
] {
    let profile_dir = ($env.LOCALAPPDATA | path join "chrome-profiles" $profile)
    if not ($profile_dir | path exists) {
        mkdir $profile_dir
    }
    let chrome = (which chrome | get path.0?)
    if ($chrome | is-empty) {
        error make { msg: "chrome.exe not found. Try: scoop install extras/googlechrome" }
    }
    print $"chrome-yatcy: port=($port), profile=($profile)"
    print $"  exe : ($chrome)"
    print $"  data: ($profile_dir)"
    let chrome_args = ([
        $"--remote-debugging-port=($port)"
        "--remote-allow-origins=*"
        $"--user-data-dir=($profile_dir)"
        "--no-first-run"
        "--no-default-browser-check"
    ] ++ $extra)
    job spawn { ^$chrome ...$chrome_args } | ignore
}

# --- Dotfiles auto-update (background git-prompt) ---
$env.DOTCLI_JOB_ID = null

# --- Build outdated check (background) ---
$env.BUILD_CHECK_JOB_ID = null

# background job は結果を `job send 0 --tag <tag>` で main thread (job id 0) に送る。
# `job send` の位置引数は送信先 job id で tag ではない。tag なしの `job recv` は全 tag を受け取るので、
# 回収側も必ず `--tag` を付ける。tag: 1 = git-prompt, 2 = build --check
#
# $env.<var> の job id の結果を回収し、format closure で整形して 1 回だけ表示する。
# 回収済み or job 終了後は env を -1 にし、以降のプロンプトでは job recv / job list を走らせない
def --env collect-job-message [var: string, tag: int, format: closure] {
    let id = ($env | get $var)
    if $id == null or $id == -1 { return }
    mut msg = try { job recv --tag $tag --timeout 0sec } catch { null }
    if $msg == null {
        if (job list | where id == $id | is-not-empty) { return }
        # recv と job list の間に send して終了した job の結果を取りこぼさないよう、もう一度回収する
        $msg = try { job recv --tag $tag --timeout 0sec } catch { null }
    }
    if $msg != null {
        let out = (do $format ($msg | str trim))
        if ($out | is-not-empty) { print $out }
    }
    load-env {$var: -1}
}

# dotcli git-prompt の機械向けトークン (updated:N:areas / dirty / error / 空) を
# pwsh/profile.ps1 の prompt hook と同じ文言・色に整形する。未知のトークンと空は表示しない (pwsh と同じ)。
# 色は pwsh の ConsoleColor Green/Yellow/Red (= SGR 92/93/91) に合わせて light_* を使う
def format-git-prompt [token: string] {
    if ($token | str starts-with 'updated:') {
        let p = ($token | str substring 8.. | split row --number 2 ':')
        let n = $p.0
        let label = if $n == '1' { 'commit' } else { 'commits' }
        let areas = ($p.1? | default '')
        $"\n(ansi light_green)✨ Dotfiles Updated! \(($n) ($label): ($areas)\)(ansi reset)"
    } else if $token == 'dirty' {
        $"\n(ansi light_yellow)⚠ Dotfiles: dirty \(behind remote\)(ansi reset)"
    } else if $token == 'error' {
        $"\n(ansi light_red)✗ Dotfiles: pull failed(ansi reset)"
    } else {
        ''
    }
}

$env.config = ($env.config | upsert hooks.pre_prompt {|cfg|
    let existing = try { $cfg.hooks.pre_prompt } catch { [] }
    $existing | append { ||
        # ① git-prompt job の結果を回収して表示（1回のみ）
        collect-job-message DOTCLI_JOB_ID 1 {|m| format-git-prompt $m }
        # ② 初回のみ job spawn（dotcli が PATH にある場合のみ）
        if ($env.DOTCLI_JOB_ID == null and $env.DOTCLI_AVAILABLE) {
            let dot = $env.DOT
            $env.DOTCLI_JOB_ID = (job spawn {
                let r = (^dotcli git-prompt $dot | complete)
                if $r.exit_code == 0 { $r.stdout | job send 0 --tag 1 }
            })
        }
        # ③ build --check の結果を回収して表示（1回のみ）
        collect-job-message BUILD_CHECK_JOB_ID 2 {|m| $m }
        # ④ 初回のみ build --check を job spawn
        if ($env.BUILD_CHECK_JOB_ID == null and $env.DOTCLI_AVAILABLE) {
            $env.BUILD_CHECK_JOB_ID = (job spawn {
                let r = (^dotcli build --check | complete)
                if $r.exit_code != 0 {
                    let out = ($r.stdout + $r.stderr)
                    if ($out | str trim | is-not-empty) { $out | job send 0 --tag 2 }
                }
            })
        }
    }
})
