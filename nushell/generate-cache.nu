# starship/zoxide の init スクリプトをキャッシュ生成
let cache_dir = ($nu.default-config-dir | path join 'cache')
mkdir $cache_dir

starship init nu | save -f ($cache_dir | path join 'starship.nu')
zoxide init nushell | save -f ($cache_dir | path join 'zoxide.nu')

print $"Cache generated in ($cache_dir)"
