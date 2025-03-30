#!/bin/sh

# スクリプトのディレクトリパスを取得
SCRIPT_DIR="$( cd "$( dirname "$0" )" && pwd )"

# VS Code設定ディレクトリを作成
mkdir -p ~/.vscode-server/data/Machine/
mkdir -p ~/.vscode-server/extensions/

# VS Code設定をコピー
echo "VSCode設定をコピーしています..."
cp "$SCRIPT_DIR/settings/vscode.json" ~/.vscode-server/data/Machine/settings.json

# キーバインド設定をコピー
echo "キーバインド設定をコピーしています..."
cp "$SCRIPT_DIR/settings/keybindings.json" ~/.vscode-server/data/Machine/keybindings.json

# 拡張機能のインストール
echo "拡張機能をインストールしています..."
cat "$SCRIPT_DIR/.vscode/extensions" | while read -r line
do
  if [ ! -z "$line" ]; then
    echo "Installing $line"
    code --install-extension "$line" || echo "Failed to install $line"
  fi
done

echo "dotfilesのセットアップが完了しました!"