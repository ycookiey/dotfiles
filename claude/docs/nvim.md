# Neovim の罠 (Windows)

## `exepath()` / 外部コマンドの解決が 'shell' で変わる

- Windows の `exepath('dart')` 等は 'shell' の設定で返す file が変わる。sh 系なら拡張子の無い sh script (Flutter SDK の `bin/dart` 等) を返し、`vim.system` で ENOENT。pwsh なら `.bat` / `.cmd` を返して動くが、batch の起動が遅い (dart.bat 2.35 s vs dart.exe 0.93 s)
- 外部コマンドが「動かない / 遅い」を調べるときは、まず使っている 'shell' と `exepath()` の実際の戻り値を確かめる。plugin (conform 等) には実体の `.exe` を `command` で直接渡すと 'shell' に依らない
