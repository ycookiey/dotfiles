# Windows ローカル TCP の罠

## 停止中ポートへのローカル接続の失敗に約2秒かかる

- 現象: `127.0.0.1` の listen していないポートへの connect が即座に失敗せず、約2秒かかる(Windows は RST 受信後も SYN を再送)
- 影響: 死活確認・起動待ちのポーリングが失敗のたびに2秒かかる。「確認→起動→起動確認」の直列で数秒単位の遅延になる
- 対処: connect timeout を短く設定。ローカルの listen 中ポートへの接続は数ms で終わるので 0.5秒で十分
  - aiohttp: `aiohttp.ClientTimeout(total=3, sock_connect=0.5)`
  - 実測: 2.04s(`ClientConnectorError`) → 0.51s(`ConnectionTimeoutError`)

## bind が WinError 10013 で拒否される

- 現象: `socket.bind(("127.0.0.1", port))` が `PermissionError: [WinError 10013] アクセス許可で禁じられた方法でソケットにアクセスしようとしました`
- 原因: Hyper-V / WSL / Docker 等が予約した除外ポート範囲(実測: 50998 が 50951-51050 内)。範囲は再起動で変わることがある
- 確認: `netsh int ipv4 show excludedportrange protocol=tcp`
- 固定ポートを選ぶ前に確認する(予約表 ports.md と併用)
