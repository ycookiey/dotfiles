# bwup — Bitwarden CLI 常駐化(localhost:8087)
#
# 1 日 1 回起動: master password 入力 → unlock → bw serve --port 8087 前面 blocking。
# Claude 等の外部ツールは以降 BW_SESSION 不要で curl http://localhost:8087/... で
# password 取得可能。Ctrl+C で serve 停止 = session 破棄 = 自動 lock 相当。
#
# nushell + bw は job エラー(nu::shell::job::not_found)を起こすため、bin/*.ps1 wrap
# 経由(dotcli の [[script]] エントリ)で全 shell から pwsh -File 経由で呼ばれる。

$ErrorActionPreference = 'Stop'

bw lock *>$null

$env:BW_SESSION = (bw unlock --raw)
if (-not $env:BW_SESSION) {
    Write-Error 'bw unlock failed'
    exit 1
}

bw serve --port 8087
