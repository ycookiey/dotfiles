# starship/zoxide の init スクリプトをキャッシュ生成
#
# デフォルトは stale チェック (exe mtime > cache mtime のときのみ再生成)。
# --force で強制再生成。
#
# 後処理:
#   - 埋め込まれた starship.exe path を $env.STARSHIP_EXE 参照に置換
#     (ユーザ名・SCOOP_HOME に非依存。env.nu で動的に解決)
#   - PROMPT_MULTILINE_INDICATOR は静的なので subprocess を文字列リテラルに置換

def main [--force] {
    let cache_dir = ($nu.default-config-dir | path join 'cache')
    mkdir $cache_dir

    let starship_exe = (resolve-exe 'starship')
    let zoxide_exe = (resolve-exe 'zoxide')

    let starship_cache = ($cache_dir | path join 'starship.nu')
    let zoxide_cache = ($cache_dir | path join 'zoxide.nu')

    let starship_stale = $force or (is-stale $starship_exe $starship_cache)
    let zoxide_stale = $force or (is-stale $zoxide_exe $zoxide_cache)

    if $starship_stale and ($starship_exe | path exists) {
        gen-starship $starship_exe $starship_cache
    }
    if $zoxide_stale and ($zoxide_exe | path exists) {
        gen-zoxide $zoxide_exe $zoxide_cache
    }

    if $starship_stale or $zoxide_stale {
        print $"Cache regenerated \(starship: ($starship_stale), zoxide: ($zoxide_stale)\)"
    }
}

def resolve-exe [name: string]: nothing -> string {
    # 標準 scoop パスを優先 (高速)。なければ scoop prefix にフォールバック (SCOOP_HOME カスタム時)
    let standard = ($env.USERPROFILE | path join $"scoop\\apps\\($name)\\current\\($name).exe")
    if ($standard | path exists) {
        return $standard
    }
    try {
        ((^scoop prefix $name | str trim) | path join $"($name).exe")
    } catch { '' }
}

def is-stale [exe: string, cache: string]: nothing -> bool {
    if not ($cache | path exists) { return true }
    if not ($exe | path exists) { return false }
    (ls $exe | get 0.modified) > (ls $cache | get 0.modified)
}

def gen-starship [exe: string, cache: string] {
    let continuation = (^$exe prompt --continuation | complete | get stdout)
    let init = (^$exe init nu)

    # init 内に埋まる exe path を全て検出 (shim/直接 exe 両方の可能性) して $env.STARSHIP_EXE に置換
    let embedded_paths = ($init | parse --regex `\^'(?<p>[^']+\.exe)'` | get p | uniq)
    mut patched = $init
    for p in $embedded_paths {
        $patched = ($patched | str replace --all $"'($p)'" '($env.STARSHIP_EXE)')
    }
    let pattern = r#'\(\s*\^\(\$env.STARSHIP_EXE\) prompt --continuation\s*\)'#
    $patched = ($patched | str replace --regex $pattern ($continuation | to nuon))
    $patched | save -f $cache
}

def gen-zoxide [exe: string, cache: string] {
    ^$exe init nushell | save -f $cache
}
