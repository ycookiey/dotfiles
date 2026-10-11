# シンボリックリンク構成

setup.ps1 で管理。追加・変更は setup.ps1 の mkl で行う（直接コマンドで作らない）。

確認コマンド: ls -la ~/.claude/

## アプリ設定

| リンク元（使われる場所） | 実体 |
|---|---|
| ~/Documents/PowerShell/Microsoft.PowerShell_profile.ps1 | dotfiles/pwsh/profile.ps1 |
| ~/.config/wezterm/ | dotfiles/wezterm/ |
| %APPDATA%/yazi/config/ | dotfiles/yazi/ |
| %LOCALAPPDATA%/nvim/ | dotfiles/nvim/ |
| %LOCALAPPDATA%/lazygit/config.yml | dotfiles/lazygit/config.yml |
| %LOCALAPPDATA%Low/Google/Google Japanese Input/config1.db | dotfiles/google-ime-config1.db |
| ~/.wslconfig | dotfiles/wsl/wslconfig |

## Claude Code (~/.claude/)

| リンク元 | 実体 |
|---|---|
| ~/.claude/aliases.ps1 | dotfiles/aliases.ps1 |
| ~/.claude/CLAUDE.md | dotfiles/claude/CLAUDE.md |
| ~/.claude/settings.json | マージ型（dotfiles/claude/settings.json をテンプレートとしてマージ） |
| ~/.claude/keybindings.json | マージ型（dotfiles/claude/keybindings.json をテンプレートとしてマージ。bindings 配列は context 単位でマージ） |
| ~/.claude/statusline.py | dotfiles/claude/statusline.py |
| ~/.claude/statusline-rules.toml | dotfiles/claude/statusline/statusline-models.toml |
| ~/.claude/rules/ | dotfiles/claude/rules/ |
| ~/.claude/docs/ | dotfiles/claude/docs/ |
| ~/.claude/agents/ | dotfiles/claude/agents/ |
| ~/.claude/skills/<name> | dotfiles/skills/<name> ※例外的にywatchyが個別管理（setup.ps1管理外）。詳細は ywatchy.md |

## マルチアカウント (~/.claude-*)

~/.claude-* ディレクトリが存在する場合、~/.claude/ 配下の各エントリが自動ミラーリングされる。除外: .credentials*, .statusline_debug.json, settings.json, keybindings.json（settings.json と keybindings.json は個別にマージ型で管理）

## リンク作成の権限（手動で作る場合）

原則は setup.ps1 の mkl。緊急でその場に作る必要がある場合のみ下記。

- **symlink 作成は管理者権限が必要**。`ln -s` は `Operation not permitted`、`New-Item -ItemType SymbolicLink` は `Administrator privilege required` で失敗する
- **ディレクトリなら junction で代替可**: `cmd /c mklink /J <link> <target>` は権限不要。同一マシン内のディレクトリに限る（ファイル・ネットワーク越しは不可）
- **junction は `readlink` で見えない**。`(Get-Item <path>).LinkType` / `.Target` で確認する
- 恒久化するなら setup.ps1 か ywatchy（ywatchy.md）へ寄せる。手動リンクは環境再構築で消える

## 書込時の注意

~/.claude 配下は symlink。symlink越しの Write/新規作成は拒否される。書込前に `readlink -f <対象>` で実体パス（dotfiles 配下）を解決し、その絶対パスで Edit/Write する。上表が主な実体マッピング。