#!/bin/bash

# dotfilesディレクトリのパスを取得
DOTFILES_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# VS Code設定ディレクトリを作成
mkdir -p ~/.vscode-server/data/Machine/
mkdir -p ~/.vscode-server/extensions/

# VS Code設定をコピー
echo "VSCode設定をコピーしています..."
cp "$DOTFILES_DIR/settings/vscode.json" ~/.vscode-server/data/Machine/settings.json

# キーバインド設定をコピー
echo "キーバインド設定をコピーしています..."
cp "$DOTFILES_DIR/settings/keybindings.json" ~/.vscode-server/data/Machine/keybindings.json

# 拡張機能のインストール
echo "拡張機能をインストールしています..."

# extensionsファイルからインストール
if [ -f "$DOTFILES_DIR/extensions" ]; then
  echo "拡張機能のインストールを開始します..."
  while read -r ext; do
    # 空行やコメント行をスキップ
    [[ -z "$ext" || "$ext" =~ ^# ]] && continue
    echo "Installing $ext"
    code --install-extension "$ext" || echo "Failed to install $ext"
  done < "$DOTFILES_DIR/extensions"
fi

echo "dotfilesのセットアップが完了しました！"