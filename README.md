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
| `install/` | Scoop・非 Scoop ツールのインストールスクリプト |
| `bin/` | ユーティリティスクリプト |
| `autohotkey/` | AHK |
| `vscode/` | VS Code |
| `git/` | Git 設定 |

## Scoop 以外のツール

Scoop で管理できないツール（`irm URL | iex` 等）は `install/tools.json` で宣言的に管理する。

```json
[
  {
    "name": "Claude Code",
    "cmd": "claude",
    "install": "irm https://claude.ai/install.ps1 | iex"
  }
]
```

| フィールド | 説明 |
|---|---|
| `name` | 表示名 |
| `cmd` | インストール済み判定に使うコマンド名 |
| `install` | インストール用スクリプト（`iex` で実行） |

`setup.ps1` 実行時に自動で未インストールのツールのみ導入される。
