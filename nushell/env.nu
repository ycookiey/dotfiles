# env.nu — 環境変数設定（config.nu より先に読み込まれる）

$env.STARSHIP_CONFIG = 'C:\Main\Project\dotfiles\starship.toml'
$env.STARSHIP_EXE = ($env.USERPROFILE | path join 'scoop\apps\starship\current\starship.exe')
# cache/zoxide.nu が呼ぶ exe (PATH 上の scoop shim を経由しない)。解決規則は cache-key.nu の scoop-exe と同じ
# TODO: 上の STARSHIP_EXE も $env.SCOOP に従わせる (現状 USERPROFILE\scoop 固定。カスタム scoop root だと cache の呼び先が無い)
$env.ZOXIDE_EXE = ($env.SCOOP? | default ($env.USERPROFILE | path join 'scoop') | path join 'apps\zoxide\current\zoxide.exe')
$env.YAZI_FILE_ONE = ($env.USERPROFILE | path join 'scoop\apps\git\current\usr\bin\file.exe')
$env.DOT = 'C:\Main\Project\dotfiles'
$env.EDITOR = 'nvim'
$env.VISUAL = 'nvim'

# pnpm/npm が裏で cmd.exe 経由で bash を解決すると WSL bash がヒットする問題対策。
# script-shell を Git Bash に固定する。
$env.NPM_CONFIG_SCRIPT_SHELL = ($env.USERPROFILE | path join 'scoop\apps\git\current\usr\bin\bash.exe')

# `bash` を解決する際 System32\bash.exe (WSL) より Git Bash を優先するため、
# Git Bash の usr/bin を PATH の先頭に prepend する。
# pnpm scripts が呼ぶ shebang スクリプト (#!/usr/bin/env bash) もこの解決を経由する。
$env.PATH = ($env.PATH | split row (char esep)
    | prepend ($env.USERPROFILE | path join 'scoop\apps\git\current\usr\bin')
    | uniq)
