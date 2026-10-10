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

## WSL で動いているサーバーのポートが Windows の 127.0.0.1 でも塞がる

- 現象: WSL(NAT、`.wslconfig` に networkingMode の指定なし)で 4300 番のサーバーが動いている間、Windows で同じ 4300 番に listen すると `EADDRINUSE 127.0.0.1:4300`(2026-10 実測)
- 原因(推定): WSL2 の localhost 転送(`localhostForwarding` 既定 true)が、WSL 側で listen したポートを Windows の localhost にも開く
- 影響: 同じ固定ポートを使うツールを WSL と Windows で同時に動かせない。相手が別 session だと気づきにくい
- 対処: 片方のポートをずらす。または同時に動かさない(lock を共有する)

## 短時間に大量のローカル接続を作ると TIME_WAIT が動的ポートを埋める

- Windows は TIME_WAIT の再利用が効かず、待ち(既定 約 2 分)の間ポートを占有する。動的ポートは既定 16,384 個(49152-65535)
- 実測: Vite dev + @cloudflare/vite-plugin に Playwright で数百ページを開くと、3〜4 分で `connect EADDRINUSE`。build + preview でも 1 回(約 100 秒)で TIME_WAIT が最大 約 10,700 個
- 確認: `netstat -an | grep -c TIME_WAIT`。Linux(WSL)では起きない
