# run-task.nu — F5 で起動するタスクランナー
# package.json (ルート + packages/*) の scripts を列挙し input list で選択
# 候補が1つなら自動実行

def run-task [] {
    let pkg_root_path = "package.json"
    if not ($pkg_root_path | path exists) {
        print "package.json が見つからない (このディレクトリで実行されている?)"
        return
    }

    let pkg_root = (try { open $pkg_root_path } catch { null })
    if $pkg_root == null {
        print "package.json の読込みに失敗"
        return
    }

    mut entries = []

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

    if ($entries | is-empty) {
        print "scripts が見つからない"
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
    } else {
        ^pnpm --filter $choice.scope run $choice.script
    }
}
