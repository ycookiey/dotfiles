# run-task.nu — F5 で起動するタスクランナー
# 以下のソースを統合してエントリ化し、input list で選択して実行する。
#   - ルート package.json の scripts
#   - packages/*/package.json の scripts (workspace)
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

            let workspace_pkgs = (try { glob packages/*/package.json } catch { [] })
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

    print $"=> ($choice.display)"
    if $choice.scope == "root" {
        ^pnpm run $choice.script
    } else if $choice.scope == "just" {
        ^just $choice.script
    } else {
        ^pnpm --filter $choice.scope run $choice.script
    }
}
