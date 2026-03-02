# agent-browser / browser-fetch

## Windows環境でのデーモン起動問題

**症状**: agent-browser コマンドが Daemon failed to start で失敗

**原因**:
1. --session オプションがデーモン起動を妨げる
2. 自動デーモン起動が動作しない環境がある

**解決策**:
- --session オプションを使用しない（デフォルトセッションを使用）
- browser-fetchスキルのスクリプト（open.sh等）は自動フォールバック対応済み
- 手動でデーモンを起動する場合: cd $(npm root -g)/agent-browser && node dist/daemon.js &