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

# 拡張機能リストからインストール
echo "拡張機能をインストールしています..."
if [ -f "$DOTFILES_DIR/.vscode/extensions.json" ]; then
  EXTENSIONS=$(cat "$DOTFILES_DIR/.vscode/extensions.json" | grep -o '"[^"]*"' | grep -v "recommendations" | tr -d '"')
  for ext in $EXTENSIONS; do
    echo "Installing $ext"
    code --install-extension "$ext" || echo "Failed to install $ext"
  done
fi

echo "dotfilesのセットアップが完了しました！"