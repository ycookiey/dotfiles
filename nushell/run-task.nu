# run-task.nu — F5 で起動するタスクランナー
# 以下のソースを統合してエントリ化し、input list で選択して実行する。
#   - ルート package.json の scripts
#   - workspace の各 package.json の scripts
#     (pnpm-workspace.yaml の packages → package.json の workspaces → packages/* の順に探す)
#   - justfile の recipes
# 直近の使用履歴を frecency でスコアリングしてディレクトリ別に並び替える。
# 候補が1つなら自動実行、複数なら fzy 選択。

const HISTORY_FILE = 'run-task-history.json'

# half-life 14日 → decay = ln(2) / (14 * 24) ≈ 0.002063 per hour
const FRECENCY_DECAY_PER_HOUR = 0.002063

def history-path [] {
    $nu.cache-dir | path join $HISTORY_FILE
}

def load-history [] {
    let p = (history-path)
    if not ($p | path exists) { return {} }
    try { open $p } catch { {} }
}

def save-history [hist: record] {
    let p = (history-path)
    mkdir ($p | path dirname)
    $hist | save -f $p
}

def now-secs [] {
    date now | format date "%s" | into int
}

def frecency-score [entry: record, now: int] {
    let age_hours = (($now - $entry.last) / 3600)
    $entry.count * (-1.0 * $FRECENCY_DECAY_PER_HOUR * $age_hours | math exp)
}

def record-usage [cwd: string, task: string] {
    let hist = (load-history)
    let now = (now-secs)
    let dir_entries = ($hist | get -o $cwd | default [])
    let updated = if ($dir_entries | any { |e| $e.task == $task }) {
        $dir_entries | each { |e|
            if $e.task == $task {
                { task: $e.task, count: ($e.count + 1), last: $now }
            } else { $e }
        }
    } else {
        $dir_entries | append { task: $task, count: 1, last: $now }
    }
    save-history ($hist | upsert $cwd $updated)
}

def sort-by-frecency [entries: list<any>, cwd: string] {
    let hist = (load-history | get -o $cwd | default [])
    if ($hist | is-empty) { return $entries }
    let now = (now-secs)
    $entries
        | each { |e|
            let h = ($hist | where task == $e.display | get -o 0)
            let score = if $h == null { 0.0 } else { (frecency-score $h $now) }
            $e | upsert _score $score
        }
        | sort-by _score --reverse
        | reject _score
}

# 実行に使うパッケージマネージャを検出する。
# 優先順: package.json の packageManager フィールド → lockfile → pnpm(既定)
def detect-pm [] {
    let pkg = (try { open package.json } catch { null })
    if $pkg != null {
        let pm_field = (try { $pkg | get packageManager } catch { null })
        if $pm_field != null {
            # "bun@1.3.10" / "pnpm@9.0.0+sha512..." → "bun" / "pnpm"
            return ($pm_field | split row '@' | first)
        }
    }
    if (('bun.lock' | path exists) or ('bun.lockb' | path exists)) { return 'bun' }
    if ('pnpm-lock.yaml' | path exists) { return 'pnpm' }
    if ('yarn.lock' | path exists) { return 'yarn' }
    if ('package-lock.json' | path exists) { return 'npm' }
    'pnpm'
}

# bash 単引用符 escape: 中の ' を '\'' に置換し、全体を ' で囲む。
# bash に単一 string を `-c` で渡すための前処理 (pm-exec 参照)。
def _bash_squote [s: string] {
    "'" + ($s | str replace --all "'" "'\\''") + "'"
}

# Windows では `^pnpm` 等が PATH 解決で .cmd shim を選ぶことがあり、
# .cmd は cmd.exe 上で動くため Ctrl+C で「Terminate batch job (Y/N)?」が
# nushell の入力を奪う。bash 経由で起動すると node 系の sh/bash shim が
# 解決されて cmd.exe を踏まない。bun / just は native .exe shim なので不要。
# 既知の制約: bash exec 境界で MSYS のパス変換が走るため、`--filter ./pkg`
# のようなパス風引数は書き換わる可能性がある。
#
# 過去に `^bash -c '"$@"' -- $pm ...$args` を使っていたが、nushell 0.110 の
# Windows external argv quoting が `'"$@"'` 内の `"` を CreateProcess 経由で
# 適切に escape しきれず、bash 側 `-c` script で `unexpected EOF` になるケース
# があった (interactive nushell + WezTerm 経路で発火)。 bash に渡す引数から
# `"` を排除するため、nushell 側で各 token を bash 単引用符 escape し、
# `-c` には escape 済の単一 string を渡す形に変更。
# 将来 nushell upgrade 時、上記 quoting bug が修正されていれば従来の
# `'"$@"'` 経路に戻すと若干 fast (escape ロジック不要) になる。
def pm-exec [pm: string, args: list<string>] {
    if $nu.os-info.name == 'windows' and ($pm in ['pnpm' 'npm' 'yarn']) {
        let cmd = ([$pm, ...$args] | each { |a| _bash_squote $a } | str join ' ')
        ^bash -c $cmd
    } else {
        run-external $pm ...$args
    }
}

# workspace の package.json の glob パターン (ディレクトリ単位) を返す。
# 1. pnpm-workspace.yaml の packages
# 2. package.json の workspaces (配列、または yarn の {packages: [...]})
# 3. どちらも無ければ packages/*
# キーがあれば空配列でもその定義に従う (packages/* には戻らない)。
# packages を持たない pnpm-workspace.yaml (catalog や設定だけ) は定義なしとして次へ進む。
def workspace-patterns [pkg_root: any] {
    let pnpm_ws = if ('pnpm-workspace.yaml' | path exists) {
        try { open pnpm-workspace.yaml | get -o packages } catch { null }
    } else { null }
    if ($pnpm_ws | describe | str starts-with 'list') { return $pnpm_ws }

    let ws = (try { $pkg_root | get -o workspaces } catch { null })
    let ws = if ($ws | describe | str starts-with 'record') { $ws | get -o packages } else { $ws }
    if ($ws | describe | str starts-with 'list') { return $ws }

    ['packages/*']
}

# workspace パターンに一致するディレクトリの package.json を返す (絶対パス、重複なし)。
# TODO: `!` で始まる除外パターンは未対応。現在は無視しているため、除外したパッケージも候補に出る
def workspace-package-files [pkg_root: any] {
    let root_pkg = ('package.json' | path expand)
    workspace-patterns $pkg_root
        | where { |p| ($p | describe) == 'string' and not ($p | str starts-with '!') }
        | each { |p|
            # glob では `\` がエスケープ扱いになり得るので `/` で連結する
            let dir = ($p | str replace --regex '^(\./)+' '' | str trim --right --char '/')
            let pattern = if ($dir | is-empty) or $dir == '.' { 'package.json' } else { $"($dir)/package.json" }
            if ($pattern =~ '[*?\[{]') {
                try { glob $pattern --exclude ['**/node_modules/**'] } catch { [] }
            } else if ($pattern | path exists) {
                # nushell 0.110 の glob は --exclude を付けるとワイルドカードの無いパターンに何も返さない
                [($pattern | path expand)]
            } else { [] }
        }
        | flatten
        | uniq
        | where { |f| $f != $root_pkg }
}

def run-task [] {
    mut entries = []

    let pkg_root_path = "package.json"
    if ($pkg_root_path | path exists) {
        let pkg_root = (try { open $pkg_root_path } catch { null })
        if $pkg_root != null {
            let root_scripts = (try { $pkg_root | get scripts | columns } catch { [] })
            for s in $root_scripts {
                $entries = ($entries | append { display: $"root: ($s)", scope: "root", script: $s })
            }

            let workspace_pkgs = (workspace-package-files $pkg_root)
            for p in $workspace_pkgs {
                let pkg = (try { open $p } catch { null })
                if $pkg == null { continue }
                let name = (try { $pkg | get name } catch { null })
                if $name == null { continue }
                let scripts = (try { $pkg | get scripts | columns } catch { [] })
                for s in $scripts {
                    $entries = ($entries | append { display: $"($name): ($s)", scope: $name, script: $s })
                }
            }
        }
    }

    let has_justfile = (['justfile', 'Justfile', '.justfile'] | any { |f| ($f | path exists) })
    if $has_justfile {
        let just_tasks = (try {
            ^just --summary | str trim | split row ' ' | where { |t| ($t | str length) > 0 }
        } catch { [] })
        for t in $just_tasks {
            $entries = ($entries | append { display: $"just: ($t)", scope: "just", script: $t })
        }
    }

    if ($entries | is-empty) {
        print "タスクが見つからない (package.json / justfile が必要)"
        return
    }

    let cwd = (pwd)
    let sorted = (sort-by-frecency $entries $cwd)

    let choice = if ($sorted | length) == 1 {
        $sorted | first
    } else {
        $sorted | input list --fuzzy --display display "Run task"
    }

    if $choice == null { return }

    record-usage $cwd $choice.display

    if $choice.scope == "just" {
        print $"=> ($choice.display)"
        ^just $choice.script
        return
    }

    let pm = (detect-pm)
    print $"=> ($choice.display)  [($pm)]"
    if $choice.scope == "root" {
        pm-exec $pm [run $choice.script]
    } else {
        # workspace filter: pnpm/bun は --filter、npm は -w、yarn は workspace
        match $pm {
            "npm" => { pm-exec $pm [-w $choice.scope run $choice.script] }
            "yarn" => { pm-exec $pm [workspace $choice.scope run $choice.script] }
            _ => { pm-exec $pm [--filter $choice.scope run $choice.script] }
        }
    }
}
