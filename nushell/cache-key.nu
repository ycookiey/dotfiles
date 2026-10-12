# starship/zoxide init キャッシュの鮮度判定キー
#
# generate-cache.nu (生成側) と config.nu (起動時の inline 判定) の両方が use する。
# キーはキャッシュ先頭行に `# key: <exe 実パス> v<GENERATOR_VERSION>` として書かれ、
# 先頭行とこのキーの文字列比較で stale を判定する。
#   - exe 実パスは scoop の current junction を path expand した版ディレクトリ配下のパス。
#     scoop は exe の mtime を archive のまま保持するため、mtime 比較では更新を検知できない
#   - exe が無いときは junction 経由の未解決パスになり、後から導入されるとキーが変わる
#   - generate-cache.nu の後処理を変えたら GENERATOR_VERSION を上げる

export const GENERATOR_VERSION = 1

# scoop の current junction 経由の exe パス (STARSHIP_EXE/ZOXIDE_EXE と同じ解決規則)
export def scoop-exe [name: string]: nothing -> string {
    let scoop_root = ($env.SCOOP? | default ($env.USERPROFILE | path join 'scoop'))
    $scoop_root | path join 'apps' $name 'current' $"($name).exe"
}

export def expected-key [name: string]: nothing -> string {
    $"# key: (scoop-exe $name | path expand) v($GENERATOR_VERSION)"
}

# キャッシュが無い・先頭行がキーと異なるとき true
export def cache-stale [name: string, cache: string]: nothing -> bool {
    let head = (try { open --raw $cache | lines | first } catch { '' })
    $head != (expected-key $name)
}
