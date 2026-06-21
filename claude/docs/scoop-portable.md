# Scoop + PortableApps wrapper 起動

scoop 経由の PortableApps 系アプリ (`xxx-portable.exe` で起動するもの, e.g. `discord-portable`, `vscode-portable`, `signal-portable`) は **wrapper exe を直接起動**するのが正規。

## 罠

URI ハンドラ (`scheme:`) や Squirrel が作るスタートメニューショートカットを経由すると、wrapper が行う env/user-data-dir/cleanup setup を飛ばすため不安定 or 壊れる。

例: `discord:` URI は HKCU にバージョン入り `app\app-<ver>\Discord.exe` を直接指して登録される。Discord update で stale 化する可能性。

## 対処

ランチャー定義は wrapper exe を直接指定:
- dotcli: `type = "exe"`, `path = '{USERPROFILE}\scoop\apps\<app>\current\<app>-portable.exe'`
- PowerShell: `Start-Process "$env:USERPROFILE\scoop\apps\<app>\current\<app>-portable.exe"`

## historical artifact

Discord 本体 / Squirrel が起動時に作る `Discord Inc\Discord.lnk` 等は cold start で再生成されない one-time artifact のことがある (cold start 60 秒検証済み)。残存していれば手動削除で恒久対処。
