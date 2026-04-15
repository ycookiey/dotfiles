# env.nu — 環境変数設定（config.nu より先に読み込まれる）

$env.STARSHIP_CONFIG = 'C:\Main\Project\dotfiles\starship.toml'
$env.YAZI_FILE_ONE = ($env.USERPROFILE | path join 'scoop\apps\git\current\usr\bin\file.exe')
$env.DOT = 'C:\Main\Project\dotfiles'
