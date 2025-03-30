#!/bin/bash

# ログファイルの設定
LOG_FILE="$HOME/dotfiles-install.log"

# dotfilesディレクトリのパスを取得
DOTFILES_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# VS Code設定ディレクトリを作成
mkdir -p ~/.vscode-server/data/Machine/ 2>&1 | tee -a $LOG_FILE
mkdir -p ~/.vscode-server/extensions/ 2>&1 | tee -a $LOG_FILE

# VS Code設定をコピー
echo "VSCode設定をコピーしています..." | tee -a $LOG_FILE
cp "$DOTFILES_DIR/settings/vscode.json" ~/.vscode-server/data/Machine/settings.json 2>&1 | tee -a $LOG_FILE

# キーバインド設定をコピー
echo "キーバインド設定をコピーしています..." | tee -a $LOG_FILE
cp "$DOTFILES_DIR/settings/keybindings.json" ~/.vscode-server/data/Machine/keybindings.json 2>&1 | tee -a $LOG_FILE

# 拡張機能のインストール
echo "拡張機能をインストールしています..." | tee -a $LOG_FILE

# extensionsファイルからインストール
if [ -f "$DOTFILES_DIR/extensions" ]; then
  echo "拡張機能のインストールを開始します..." | tee -a $LOG_FILE
  while read -r ext; do
    # 空行やコメント行をスキップ
    [[ -z "$ext" || "$ext" =~ ^# ]] && continue
    echo "Installing $ext" | tee -a $LOG_FILE
    code --install-extension "$ext" 2>&1 | tee -a $LOG_FILE || echo "Failed to install $ext" | tee -a $LOG_FILE
  done < "$DOTFILES_DIR/extensions"
else
  echo "extensionsファイルが見つかりません。以下のコマンドで現在の拡張機能をエクスポートできます:" | tee -a $LOG_FILE
  echo "code --list-extensions > $DOTFILES_DIR/extensions" | tee -a $LOG_FILE
fi

echo "dotfilesのセットアップが完了しました！" | tee -a $LOG_FILE