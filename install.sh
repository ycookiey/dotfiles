#!/bin/sh

# スクリプトのディレクトリパスを取得
SCRIPT_DIR="$( cd "$( dirname "$0" )" && pwd )"
LOG_FILE="$SCRIPT_DIR/install.log"

# ログファイルを初期化
date > "$LOG_FILE"
echo "VSCode dotfiles インストール開始" >> "$LOG_FILE"

# VS Code設定ディレクトリを作成
echo "設定ディレクトリを作成しています..." | tee -a "$LOG_FILE"
mkdir -p ~/.vscode-server/data/Machine/
mkdir -p ~/.vscode-server/extensions/

# VS Code設定をコピー
echo "VSCode設定をコピーしています..." | tee -a "$LOG_FILE"
if cp "$SCRIPT_DIR/settings/vscode.json" ~/.vscode-server/data/Machine/settings.json; then
  echo "  ✓ 設定ファイルのコピー成功" | tee -a "$LOG_FILE"
else
  echo "  ✗ 設定ファイルのコピー失敗" | tee -a "$LOG_FILE"
fi

# キーバインド設定をコピー
echo "キーバインド設定をコピーしています..." | tee -a "$LOG_FILE"
if cp "$SCRIPT_DIR/settings/keybindings.json" ~/.vscode-server/data/Machine/keybindings.json; then
  echo "  ✓ キーバインド設定のコピー成功" | tee -a "$LOG_FILE"
else
  echo "  ✗ キーバインド設定のコピー失敗" | tee -a "$LOG_FILE"
fi

# 拡張機能のインストール
echo "拡張機能をインストールしています..." | tee -a "$LOG_FILE"
echo "拡張機能のインストール開始: $(date)" >> "$LOG_FILE"

INSTALLED_COUNT=0
FAILED_COUNT=0

cat "$SCRIPT_DIR/.vscode/extensions" | while read -r line
do
  if [ ! -z "$line" ]; then
    echo "Installing $line" | tee -a "$LOG_FILE"
    if code --install-extension "$line"; then
      echo "  ✓ $line" >> "$LOG_FILE"
      INSTALLED_COUNT=$((INSTALLED_COUNT+1))
    else
      echo "  ✗ Failed to install $line" | tee -a "$LOG_FILE"
      FAILED_COUNT=$((FAILED_COUNT+1))
    fi
  fi
done

# インストール結果のサマリーをログに追加
echo "拡張機能のインストール完了: $(date)" >> "$LOG_FILE"
echo "インストール成功: $INSTALLED_COUNT" >> "$LOG_FILE"
echo "インストール失敗: $FAILED_COUNT" >> "$LOG_FILE"

echo "dotfilesのセットアップが完了しました!" | tee -a "$LOG_FILE"
echo "詳細は $LOG_FILE を確認してください"