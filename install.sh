#!/bin/sh

# dotfilesディレクトリのパスを取得
DOTFILES_DIR="$(cd "$(dirname "$0")" && pwd)"

# VS Code設定ディレクトリを作成
mkdir -p ~/.vscode-server/data/Machine/
mkdir -p ~/.vscode-server/extensions/

# VS Code設定をコピー
echo "VSCode設定をコピーしています..."
cp "$DOTFILES_DIR/settings/vscode.json" ~/.vscode-server/data/Machine/settings.json

# キーバインド設定をコピー
echo "キーバインド設定をコピーしています..."
cp "$DOTFILES_DIR/settings/keybindings.json" ~/.vscode-server/data/Machine/keybindings.json

# extsファイルを生成（次回起動時に自動インストールされる）
echo "拡張機能のインストール設定を作成しています..."
EXTS_FILE=~/.vscode-server/data/Machine/extensions.json
echo "{" > $EXTS_FILE
echo "  \"recommendations\": [" >> $EXTS_FILE

# extensionsの内容をパース
if [ -f "$DOTFILES_DIR/.vscode/extensions.json" ]; then
  extensions=$(cat "$DOTFILES_DIR/.vscode/extensions.json" | grep -o '"[^"]*"' | grep -v "recommendations" | tr -d '"')
  first=true
  for ext in $extensions; do
    if [ "$first" = true ]; then
      echo "    \"$ext\"" >> $EXTS_FILE
      first=false
    else
      echo "    ,\"$ext\"" >> $EXTS_FILE
    fi
  done
fi

echo "  ]" >> $EXTS_FILE
echo "}" >> $EXTS_FILE

# フラグファイルを作成して次回起動時に認識させる
touch ~/.vscode-server/.dotfiles-installed

echo "dotfilesのセットアップが完了しました！"