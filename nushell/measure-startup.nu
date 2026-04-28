#!/usr/bin/env nu
# nushell 起動プロファイラ。
#   [1] 全体コールドスタート (nu -c exit) を 3 回計測し中央値を表示
#   [2] config.nu / env.nu 内の各セクションを --no-config-file で隔離計測
# 使い方: nu nushell/measure-startup.nu

def sections []: nothing -> table<label: string, code: string> {
    let dot = 'C:\Main\Project\dotfiles'
    [
        [label                    code];
        ["env.nu"                 $"source '($dot)\\nushell\\env.nu'"]
        ["cache/starship.nu"      $"source '($dot)\\nushell\\cache\\starship.nu'"]
        ["cache/zoxide.nu"        $"source '($dot)\\nushell\\cache\\zoxide.nu'"]
        ["generated-aliases.nu"   $"source '($dot)\\nushell\\generated-aliases.nu'"]
        ["run-task.nu"            $"source '($dot)\\nushell\\run-task.nu'"]
        ["which dotcli"           "which dotcli | is-not-empty | ignore"]
    ]
}

def measure [code: string]: nothing -> duration {
    let r = (^nu --no-config-file -c $"timeit { ($code) }" | complete)
    if $r.exit_code != 0 {
        return 0ns
    }
    $r.stdout | str trim | into duration
}

def median [list: list<duration>]: nothing -> duration {
    let sorted = ($list | sort)
    $sorted | get (($sorted | length) // 2)
}

def main [] {
    print "== nushell startup profiler =="
    print ""

    print "[1] コールドスタート計測 (3 回中央値)"
    let baseline = (0..2 | each { |_| timeit { ^nu --no-config-file -c 'exit' } })
    let full = (0..2 | each { |_| timeit { ^nu -c 'exit' } })
    print ({
        baseline_no_config: (median $baseline)
        full_with_config: (median $full)
        config_overhead: ((median $full) - (median $baseline))
    })
    print ""

    print "[2] セクション別計測 (--no-config-file で隔離、降順)"
    sections | each { |s|
        {section: $s.label, time: (measure $s.code)}
    } | sort-by time --reverse | print
}
