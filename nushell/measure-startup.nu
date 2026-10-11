#!/usr/bin/env nu
# nushell 起動プロファイラ。計測は hyperfine (-N, --warmup) に委譲し、無ければ timeit で wall-clock を測る。
#   [1] コールドスタート: `nu -n -c exit` (config なし) と
#       `nu --env-config <root>/nushell/env.nu --config <root>/nushell/config.nu -c exit` の差を config_overhead とする。
#       `nu -c` だけでは user の env.nu/config.nu を読まない。`-n` は --config より優先されるので full 側に付けない
#   [2] ファイル別: `nu -n -c "source '<file>'"` のプロセス時間から `nu -n -c ''` を引く。
#       source/use は parse 時に解決されるため、timeit { source X } では parse コストが抜ける。プロセス時間なら含まれる
#
# 副作用の注意: config.nu を読む計測 ([1] full, [2] config.nu) は config.nu の起動処理をそのまま走らせる。
#   dotcli が PATH にあれば `dotcli sync` を job spawn し (install/*.json 等を書く)、nu の終了でその途中で kill される。
#   cache/ が stale なら generate-cache.nu も spawn する。dotcli を PATH から外して実行すること
#
# 使い方: nu -n nushell/measure-startup.nu [--root <repo>] [--runs N] [--warmup N]

# このスクリプトのディレクトリ (symlink 経由なら path expand で実体に解決してから親を repo root とする)
const SELF_DIR = path self .

def main [
    --root: path       # repo root (既定: このスクリプトの実体の親ディレクトリ)
    --runs: int        # 1 コマンドあたりの計測回数 (既定: hyperfine の既定。timeit fallback では 10)
    --warmup: int = 3  # hyperfine の warmup 回数 (timeit fallback でも同数を捨てる)
] {
    let root = ($root | default ($SELF_DIR | path expand | path dirname) | path expand)
    let nu_dir = ($root | path join 'nushell')
    if not ($nu_dir | path exists) {
        error make { msg: $"($nu_dir) が無い。--root に repo root を渡す" }
    }

    print "== nushell startup profiler =="
    print $"root: ($root)"
    print "注意: config.nu を読む計測は dotcli sync 等の background job を spawn する (書込み途中で kill される)。dotcli を PATH から外して実行すること"
    if (which dotcli | is-not-empty) {
        print $"(ansi yellow)警告: dotcli が PATH にある \((which dotcli | get path.0)\)(ansi reset)"
    }
    print ""

    let env_nu = (nu-path $nu_dir 'env.nu')
    let config_nu = (nu-path $nu_dir 'config.nu')
    let cold = [
        [key args];
        [baseline_no_config ['-n' '-c' 'exit']]
        [full_with_config ['--env-config' $env_nu '--config' $config_nu '-c' 'exit']]
    ]
    let section_rows = (sections $nu_dir)
    let section_cmds = ($section_rows | each {|s| {key: $s.label, args: ['-n' '-c' $s.code]} })
    let empty = {key: '(empty)', args: ['-n' '-c' '']}
    let cmds = ($cold | append $empty | append $section_cmds)

    # 失敗するコマンドを 0ns として扱わない。hyperfine の中断メッセージは nu の stderr を含まないので先に 1 回ずつ実行する
    for c in $cmds { preflight $c.key $c.args }

    let t = if (which hyperfine | is-not-empty) {
        print $"計測: hyperfine -N --warmup ($warmup)"
        measure-hyperfine $cmds $warmup $runs
    } else {
        print $"計測: hyperfine が無いので timeit \(wall-clock, ($runs | default 10) 回平均\)"
        measure-timeit $cmds $warmup ($runs | default 10)
    }
    let mean = {|key| $t | where key == $key | get 0.mean }
    print ""

    print "[1] コールドスタート (平均)"
    print {
        baseline_no_config: (do $mean baseline_no_config)
        full_with_config: (do $mean full_with_config)
        config_overhead: ((do $mean full_with_config) - (do $mean baseline_no_config))
    }
    print ""

    print "[2] ファイル別 (nu -n -c \"source <file>\" - nu -n -c '', parse 込み、降順)"
    let base = (do $mean '(empty)')
    let measured = ($section_rows | each {|s|
        let row = ($t | where key == $s.label | first)
        {section: $s.label, time: ($row.mean - $base), stddev: $row.stddev}
    })
    # config.nu 本体 (PATH 加工、keybindings、hooks、job spawn 等) = env.nu+config.nu から env.nu と config.nu が読むファイルを引いた値
    let total = ($measured | where section == 'config.nu (env.nu 込み)' | get 0.time)
    let parts = ($measured | where section in (config-sourced-labels) or section == 'env.nu' | get time | math sum)
    $measured
    | append {section: 'config.nu 本体 (差し引き)', time: ($total - $parts), stddev: null}
    | sort-by time --reverse
    | print
}

# nu のコード中に埋め込む path。区切りを / にして hyperfine の引数分割 (shell-words) の \ エスケープを避ける
def nu-path [dir: string, name: string]: nothing -> string {
    $dir | path join $name | str replace --all '\' '/'
}

# config.nu が use/source するファイル (sections の label と一致させる)
def config-sourced-labels []: nothing -> list<string> {
    ['cache/starship.nu' 'cache/zoxide.nu' 'generated-aliases.nu' 'run-task.nu' 'cache-key.nu']
}

# 各行は `nu -n -c <code>` で実行される。module は config.nu と同じく use で読む
def sections [nu_dir: string]: nothing -> table<label: string, code: string> {
    let p = {|name| nu-path $nu_dir $name }
    [
        [label                       code];
        ['env.nu'                    $"source '(do $p 'env.nu')'"]
        ['config.nu (env.nu 込み)'    $"source '(do $p 'env.nu')'; source '(do $p 'config.nu')'"]
        ['cache/starship.nu'         $"use '(do $p 'cache/starship.nu')'"]
        ['cache/zoxide.nu'           $"source '(do $p 'cache/zoxide.nu')'"]
        ['generated-aliases.nu'      $"source '(do $p 'generated-aliases.nu')'"]
        ['run-task.nu'               $"source '(do $p 'run-task.nu')'"]
        ['cache-key.nu'              $"use '(do $p 'cache-key.nu')' cache-stale"]
        ['which dotcli'              'which dotcli | is-not-empty | ignore']
    ]
}

def preflight [key: string, args: list<string>] {
    let r = (^$nu.current-exe ...$args | complete)
    if $r.exit_code != 0 {
        print --stderr $r.stderr
        error make { msg: $"計測対象が失敗した: ($key) \(exit ($r.exit_code)\): nu ($args | str join ' ')" }
    }
}

# hyperfine -N のコマンド文字列は shell-words で分割されるので、各引数を "..." で囲み \ " $ ` をエスケープする
def sh-quote [s: string]: nothing -> string {
    let e = ($s | str replace --all '\' '\\' | str replace --all '"' '\"' | str replace --all '$' '\$' | str replace --all '`' '\`')
    $'"($e)"'
}

def sec-to-duration [s: float]: nothing -> duration {
    $s * 1_000_000_000 | math round | into int | into duration
}

def measure-hyperfine [cmds: table, warmup: int, runs: any]: nothing -> table<key: string, mean: duration, stddev: duration> {
    let exe = (sh-quote $nu.current-exe)
    let json = (mktemp -t --suffix .json)
    let names = ($cmds | each {|c| ['--command-name' $c.key] } | flatten)
    let commands = ($cmds | each {|c| ([$exe] | append ($c.args | each {|a| sh-quote $a }) | str join ' ') })
    let run_args = if $runs == null { [] } else { ['--runs' ($runs | into string)] }
    let r = (^hyperfine -N --warmup ($warmup | into string) ...$run_args --style basic --export-json $json ...$names ...$commands | complete)
    if $r.exit_code != 0 {
        rm -f $json
        print --stderr $r.stdout $r.stderr
        error make { msg: $"hyperfine が失敗した \(exit ($r.exit_code)\)" }
    }
    let results = (open $json | get results)
    rm -f $json
    $cmds | enumerate | each {|e|
        let res = ($results | get $e.index)
        # hyperfine 2.x は summary.time_wall_clock、1.x は results 直下に mean/stddev (秒)
        let s = if ('summary' in ($res | columns)) { $res.summary.time_wall_clock } else { $res }
        {key: $e.item.key, mean: (sec-to-duration $s.mean), stddev: (sec-to-duration ($s.stddev? | default 0.0))}
    }
}

def measure-timeit [cmds: table, warmup: int, runs: int]: nothing -> table<key: string, mean: duration, stddev: duration> {
    $cmds | each {|c|
        for _ in 0..<$warmup { ^$nu.current-exe ...$c.args | complete | ignore }
        let ns = (0..<$runs | each {|_| timeit { ^$nu.current-exe ...$c.args | complete | ignore } | into int })
        {key: $c.key, mean: ($ns | math avg | math round | into int | into duration), stddev: ($ns | math stddev | math round | into int | into duration)}
    }
}
