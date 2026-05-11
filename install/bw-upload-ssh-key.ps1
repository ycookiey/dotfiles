# 鍵を生成した端末で1度だけ実行する: ~/.ssh/id_ed25519 を Bitwarden に Secure Note として保存
# 別端末では install/bw-secrets.ps1 が同じ Name で取得する
$ErrorActionPreference = "Stop"

$Name = 'GitHub SSH Key'
$KeyPath = "$HOME\.ssh\id_ed25519"

if (!(Test-Path $KeyPath)) {
    Write-Host "鍵がない: $KeyPath" -ForegroundColor Red
    exit 1
}

$status = (bw status 2>$null | ConvertFrom-Json).status
if ($status -eq 'unauthenticated') {
    $env:BW_SESSION = bw login --raw
} elseif ($status -eq 'locked') {
    $env:BW_SESSION = bw unlock --raw
}
if (!$env:BW_SESSION) {
    Write-Host "Bitwarden 認証失敗" -ForegroundColor Red
    exit 1
}

# sync で疎通確認。invalid_grant 等 token 失効時は logout して login し直す
bw sync 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "sync 失敗 — token失効と判断、logout → login し直す" -ForegroundColor Yellow
    bw logout 2>&1 | Out-Null
    $env:BW_SESSION = bw login --raw
    if (!$env:BW_SESSION) {
        Write-Host "再ログイン失敗" -ForegroundColor Red
        exit 1
    }
    bw sync | Out-Null
}
$existing = bw list items --search $Name | ConvertFrom-Json | Where-Object { $_.name -eq $Name }
if ($existing) {
    Write-Host "既存: $($existing.id) — Notes を更新" -ForegroundColor Yellow
    $keyText = Get-Content $KeyPath -Raw
    $obj = $existing | Select-Object -First 1
    $obj.notes = $keyText
    $obj | ConvertTo-Json -Depth 10 -Compress | bw encode | bw edit item $obj.id | Out-Null
    Write-Host "更新完了" -ForegroundColor Green
} else {
    $keyText = Get-Content $KeyPath -Raw
    $item = @{
        type = 2  # Secure Note
        name = $Name
        notes = $keyText
        secureNote = @{ type = 0 }
        folderId = $null
        favorite = $false
        fields = @()
        reprompt = 0
    } | ConvertTo-Json -Depth 10
    $id = ($item | bw encode | bw create item | ConvertFrom-Json).id
    Write-Host "登録完了: $id" -ForegroundColor Green
}
bw sync | Out-Null
