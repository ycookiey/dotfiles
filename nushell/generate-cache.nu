# starship/zoxide の init スクリプトをキャッシュ生成
#
# デフォルトは stale チェック: キャッシュ先頭行の `# key: ...` が cache-key.nu の期待キーと
# 異なるとき (exe の版が変わった・GENERATOR_VERSION が上がった・キャッシュが無い) のみ再生成。
# --force で強制再生成。
#
# 後処理 (変えたら cache-key.nu の GENERATOR_VERSION を上げる):
#   - starship: 埋め込まれた exe path (scoop shim 等) を $env.STARSHIP_EXE 参照に置換
#     (ユーザ名・SCOOP_HOME に非依存。env.nu で動的に解決)
#   - starship: PROMPT_MULTILINE_INDICATOR は静的なので subprocess を文字列リテラルに置換
#   - zoxide: `^zoxide` (PATH 経由で shim を通る) を $env.ZOXIDE_EXE 参照に置換
#
# exe が無いときもコメントと key だけの stub を書く (config.nu の use/source を壊さない)。
# 保存は一時ファイル経由の mv で置換し、job kill 等で書込み途中の cache が残らないようにする。

use cache-key.nu [scoop-exe expected-key cache-stale]

def main [--force] {
    let cache_dir = ($nu.default-config-dir | path join 'cache')
    mkdir $cache_dir

    let starship_cache = ($cache_dir | path join 'starship.nu')
    let zoxide_cache = ($cache_dir | path join 'zoxide.nu')

    let starship_stale = $force or (cache-stale 'starship' $starship_cache)
    let zoxide_stale = $force or (cache-stale 'zoxide' $zoxide_cache)

    if $starship_stale {
        let exe = (scoop-exe 'starship')
        let body = if ($exe | path exists) { gen-starship $exe } else { stub 'starship' $exe }
        write-cache $starship_cache (expected-key 'starship') $body
    }
    if $zoxide_stale {
        let exe = (scoop-exe 'zoxide')
        let body = if ($exe | path exists) { gen-zoxide $exe } else { stub 'zoxide' $exe }
        write-cache $zoxide_cache (expected-key 'zoxide') $body
    }

    if $starship_stale or $zoxide_stale {
        print $"Cache regenerated \(starship: ($starship_stale), zoxide: ($zoxide_stale)\)"
    }
}

def stub [name: string, exe: string]: nothing -> string {
    $"# ($name) not found \(($exe)\): stub so that config.nu still parses\n"
}

# 先頭行に key を付け、プロセス固有の一時ファイルに書いてから置換する
# (同時に起動した nu が同じ一時ファイルを書き合わないよう pid を含める)
def write-cache [cache: string, key: string, body: string] {
    let tmp = $"($cache).($nu.pid).tmp"
    $"($key)\n($body)" | save -f $tmp
    mv -f $tmp $cache
}

def gen-starship [exe: string]: nothing -> string {
    let continuation = (^$exe prompt --continuation | complete | get stdout)
    let init = (^$exe init nu)

    # init 内に埋まる exe path を全て検出 (shim/直接 exe 両方の可能性) して $env.STARSHIP_EXE に置換
    let embedded_paths = ($init | parse --regex `\^'(?<p>[^']+\.exe)'` | get p | uniq)
    mut patched = $init
    for p in $embedded_paths {
        $patched = ($patched | str replace --all $"'($p)'" '($env.STARSHIP_EXE)')
    }
    let pattern = r#'\(\s*\^\(\$env.STARSHIP_EXE\) prompt --continuation\s*\)'#
    $patched | str replace --regex --no-expand $pattern ($continuation | to nuon)
}

def gen-zoxide [exe: string]: nothing -> string {
    ^$exe init nushell | str replace --all '^zoxide ' '^($env.ZOXIDE_EXE) '
}
