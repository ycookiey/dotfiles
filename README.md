# dotfiles

Windows 開発環境の dotfiles。

## セットアップ

新しい PC で以下を実行:

```powershell
irm https://raw.githubusercontent.com/ycookiey/dotfiles/main/bootstrap.ps1 | iex
```

Scoop・git・pwsh のインストールからアプリ導入、シンボリックリンク作成まで自動で行う。

既存環境で設定を再適用する場合:

```powershell
pwsh -ExecutionPolicy Bypass -File setup.ps1
```

## ブートストラップの流れ

`bootstrap.ps1` → `setup.ps1` → `install/scoop.ps1` の順で実行される。

### 1. `bootstrap.ps1`（エントリポイント）

新しい PC で `irm ... | iex` を実行すると、以下の順序で処理が走る。

1. 管理者権限の確認
2. **Scoop インストール**（未インストール時のみ）
3. PATH に Scoop shims を追加
4. **git・pwsh をインストール**（clone と setup.ps1 に必要な最小セット）
5. dotfiles リポジトリを `C:\Main\Project\dotfiles` に clone
6. `setup.ps1 --scoop` を pwsh で実行

### 2. `setup.ps1`（メインセットアップ）

1. エイリアス読み込み（`pwsh/aliases.ps1`）
2. `--scoop` フラグまたは Scoop 未インストール → `install/scoop.ps1` を実行
3. 管理者権限でなければ自身を管理者として再起動し待機
4. **シンボリックリンク作成**（wezterm, yazi, nvim, nushell, lazygit, claude 等）
5. **ファイル関連付け**（Neovim を WezTerm 経由で開く拡張子の登録）
6. Claude マルチアカウントのシンボリックリンク同期
7. **dotcli**（Rust CLI）のビルドとエイリアス生成
8. **スタートアップ登録**（TaskScheduler で `startup/manager.ps1` を登録）

### 3. `install/scoop.ps1`（Scoop アプリ管理）

1. Scoop インストール（未インストール時）
2. git インストール（bucket add に必要）
3. `scoopfile.json` に定義されたバケットを追加
4. アプリのインストール（`install-order.json` による順序制御）

#### large アプリの遅延インストール

`install/install-order.json` が存在する場合、`large` に列挙されたアプリは後回しにされる。
小さいアプリを先にインストールすることで、開発環境を早く使えるようにしている。

```
install-order.json の large:
  libreoffice, android-studio, miktex,
  epic-games-launcher, llama.cpp-cu131, llvm
```

`install-order.json` が存在しない場合は `scoop import` によるフォールバック。

## 構成

| ディレクトリ | 内容 |
|---|---|
| `pwsh/` | PowerShell プロファイル・エイリアス |
| `cli/` | dotcli（Rust CLI、エイリアス生成） |
| `nvim/` | Neovim |
| `wezterm/` | WezTerm |
| `yazi/` | Yazi |
| `claude/` | Claude Code 設定 |
| `startup/` | 起動時アプリ管理 |
| `install/` | Scoop 等インストールスクリプト |
| `bin/` | ユーティリティスクリプト |
| `autohotkey/` | AHK |
| `vscode/` | VS Code |
| `git/` | Git 設定 |
