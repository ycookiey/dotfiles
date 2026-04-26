# run-task.nu — F5 で起動するタスクランナー
# 以下のソースを統合してエントリ化し、input list で選択して実行する。
#   - ルート package.json の scripts
#   - packages/*/package.json の scripts (workspace)
#   - justfile の recipes
# 候補が1つなら自動実行。

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

    let choice = if ($entries | length) == 1 {
        $entries | first
    } else {
        $entries | input list --fuzzy --display display "Run task"
    }

    if $choice == null { return }

    print $"=> ($choice.display)"
    if $choice.scope == "root" {
        ^pnpm run $choice.script
    } else if $choice.scope == "just" {
        ^just $choice.script
    } else {
        ^pnpm --filter $choice.scope run $choice.script
    }
}
