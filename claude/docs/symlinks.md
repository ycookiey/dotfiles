# シンボリックリンク構成

setup.ps1 で管理。追加・変更は setup.ps1 の mkl で行う（直接コマンドで作らない）。

確認コマンド: ls -la ~/.claude/

## アプリ設定

| リンク元（使われる場所） | 実体 |
|---|---|
| ~/Documents/PowerShell/Microsoft.PowerShell_profile.ps1 | dotfiles/pwsh/profile.ps1 |
| ~/.config/wezterm/ | dotfiles/wezterm/ |
| %APPDATA%/yazi/config/ | dotfiles/yazi/ |
| %LOCALAPPDATA%/nvim/ | dotfiles/config/nvim/ |
| %LOCALAPPDATA%/lazygit/config.yml | dotfiles/lazygit/config.yml |

## Claude Code (~/.claude/)

| リンク元 | 実体 |
|---|---|
| ~/.claude/aliases.ps1 | dotfiles/aliases.ps1 |
| ~/.claude/CLAUDE.md | dotfiles/claude/CLAUDE.md |
| ~/.claude/settings.json | マージ型（dotfiles/claude/settings.json をテンプレートとしてマージ） |
| ~/.claude/statusline.ps1 | dotfiles/claude/statusline.ps1 |
| ~/.claude/rules/ | dotfiles/claude/rules/ |
| ~/.claude/docs/ | dotfiles/claude/docs/ |
| ~/.claude/skills/ | dotfiles/claude/skills/ |
| dotfiles/claude/skills/life | C:/Main/Project/life/skills/life |

## マルチアカウント (~/.claude-*)

~/.claude-* ディレクトリが存在する場合、~/.claude/ 配下の各エントリが自動ミラーリングされる。除外: .credentials*, .statusline_cache, .statusline_debug.json, settings.json（settings.json は個別にマージ型で管理）