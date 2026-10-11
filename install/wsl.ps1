# install/wsl.ps1 — WSL 冪等セットアップ + Claude Code bash 切り替え
# 導入済み (wsl --status が exit 0 かつ Ubuntu 登録済み) なら、DISM も VM の起動もせずに戻る
$ErrorActionPreference = "Stop"
$ProgressPreference = 'SilentlyContinue'
# wsl.exe の出力を UTF-16LE でなく UTF-8 にする。これを無視する古い wsl.exe 向けに NUL の除去も残す
$env:WSL_UTF8 = '1'

$GitBash = "$HOME\scoop\apps\git\current\bin\bash.exe"
$WslBash = "C:\Windows\System32\bash.exe"
$RebootFlag = "$HOME\.claude\wsl-pending-reboot"

# wsl --status は VM を起動しない。wsl.exe が無い・機能が無効・エンジン未導入なら非 0 か例外
function Test-WslEngine {
    try { $null = wsl --status 2>&1; $LASTEXITCODE -eq 0 } catch { $false }
}

# 登録済みの Ubuntu 系ディストリ (Ubuntu, Ubuntu-24.04 等) の名前。無ければ $null。wsl -l -q も VM を起動しない
# ディストリが 1 つも無いと非 0 で案内文を出すので、exit code を見てから行単位で照合する
function Get-UbuntuDistro {
    try {
        $lines = wsl -l -q 2>$null
        if ($LASTEXITCODE -ne 0) { return $null }
        $lines -replace "`0", '' | % Trim | ? { $_ -match '^Ubuntu' } | select -First 1
    } catch { $null }
}

# ─── Step 1: Claude Code bash = git-bash ───
if ((tp $GitBash) -and ([Environment]::GetEnvironmentVariable('CLAUDE_CODE_GIT_BASH_PATH', 'User') -ne $GitBash)) {
    [Environment]::SetEnvironmentVariable('CLAUDE_CODE_GIT_BASH_PATH', $GitBash, 'User')
    wh "  CLAUDE_CODE_GIT_BASH_PATH -> git-bash" -Fo Cyan
}

# ─── fast path: エンジンと Ubuntu が導入済みなら Step 2-6 を飛ばす ───
$engineOk = Test-WslEngine
if ($engineOk -and ($distro = Get-UbuntuDistro)) {
    if (tp $RebootFlag) { rm $RebootFlag -Force }
    wh "[WSL] エンジン・$distro 導入済み" -Fo Green
    return
}

# ─── Step 2: Windows 機能有効化 ───
# Enable-WindowsOptionalFeature は既に有効でも State=Enabled / RestartNeeded=$false を返す（冪等）
wh "[WSL 1/4] Windows 機能チェック..." -Fo Cyan
$features = @(
    @{ Name = 'Microsoft-Windows-Subsystem-Linux'; Label = 'WSL' }
    @{ Name = 'VirtualMachinePlatform';             Label = 'VirtualMachinePlatform' }
)
# 機能ごとに -FeatureName で問い合わせると DISM を機能の数だけ呼ぶので、1 回の列挙から絞る
$states = @{}
Get-WindowsOptionalFeature -Online | ? FeatureName -in $features.Name | % { $states[$_.FeatureName] = "$($_.State)" }
$rebootNeeded = $false
foreach ($f in $features) {
    $before = $states[$f.Name]
    if ($before -eq 'Enabled') {
        wh "  $($f.Label): already Enabled" -Fo Green
        continue
    }
    $r = Enable-WindowsOptionalFeature -Online -FeatureName $f.Name -All -NoRestart -WarningAction SilentlyContinue
    $tag = if ($r.RestartNeeded) { '再起動必要' } else { '反映済' }
    wh "  $($f.Label): $before -> Enabled ($tag)" -Fo Yellow
    if ($r.RestartNeeded) { $rebootNeeded = $true }
}

if ($rebootNeeded) {
    "pending" | sc $RebootFlag
    wh "  再起動が必要な機能があります。再起動後に setup.ps1 を再実行してください。" -Fo Yellow
    return
}

if (tp $RebootFlag) { Remove-Item $RebootFlag -Force }

# ─── Step 3: WSL エンジン ───
# 固定版の MSI でなく最新を入れる。WSL 2.4.4 未満のエンジンは Ubuntu を appx 形式で入れ (Step 5 が必要になる)、
# 2.4.4 以上は tar 形式で --install だけで登録まで済むので、ディストリを入れる前に最新にしておく
wh "[WSL 2/4] WSL エンジン確認..." -Fo Cyan
$installed = $false
# --version は Store 版 WSL だけが持つ。無ければ inbox の wsl.exe なので --install で Store 版を入れる
$storeWsl = try { $null = wsl --version 2>&1; $LASTEXITCODE -eq 0 } catch { $false }
if ($storeWsl) {
    wh "  導入済み。更新を確認 (wsl --update)..." -Fo Yellow
    wsl --update
    # エンジン自体は動くので止めない。古いままなら Step 5 の appx 経路で登録する
    if ($LASTEXITCODE) { wh "  wsl --update 失敗 (exit $LASTEXITCODE)。今のエンジンで続行" -Fo Yellow }
} else {
    wh "  インストール中 (wsl --install --no-distribution)..." -Fo Yellow
    wsl --install --no-distribution
    # TODO: --no-distribution を持たない古い Windows の wsl.exe 向けの代替経路 (旧来の MSI 等) は未実装。今は失敗として止める
    if ($LASTEXITCODE) { throw "wsl --install --no-distribution exit $LASTEXITCODE (古い Windows では非対応。https://github.com/microsoft/WSL/releases の MSI を手動で入れる)" }
    $installed = $true
}
if (!(Test-WslEngine)) {
    "pending" | sc $RebootFlag
    wh "  WSL エンジンがまだ使えません。再起動後に setup.ps1 を再実行してください。" -Fo Yellow
    return
}
wh "  正常" -Fo Green

# ─── Step 4: Ubuntu ───
wh "[WSL 3/4] Ubuntu 確認..." -Fo Cyan
$distro = Get-UbuntuDistro
if ($distro) {
    wh "  $distro 登録済み" -Fo Green
} else {
    wh "  Ubuntu をインストール中 (wsl --install -d Ubuntu --no-launch)... (数分かかります)" -Fo Yellow
    $null = wsl --set-default-version 2 2>$null
    wsl --install -d Ubuntu --no-launch
    if ($LASTEXITCODE) { throw "wsl --install -d Ubuntu --no-launch exit $LASTEXITCODE" }
    $installed = $true
    $distro = Get-UbuntuDistro
}

# ─── Step 5: appx 形式で入ったときの登録 ───
# tar 形式は --install で登録まで済む (既定ユーザーは root。対話シェルを初めて開いたときに OOBE でユーザーを作る)。
# Step 3 の更新に失敗して古いエンジンのまま appx 形式で入ると、--no-launch では未登録のまま残るので、
# ランチャーで root を既定ユーザーにして登録する
if (!$distro) {
    wh "[WSL 4/4] Ubuntu 初期化..." -Fo Cyan
    $exe = (Get-Command "ubuntu*.exe" -CommandType Application -ea 0 | Select-Object -First 1).Source
    if (!$exe) { throw "Ubuntu が登録されておらず、ubuntu*.exe も見つからない" }
    $proc = Start-Process -FilePath $exe -ArgumentList "install --root" -Wait -NoNewWindow -PassThru
    if ($proc.ExitCode -notin @(0, -1)) {
        wh "  初期化失敗 (Code: $($proc.ExitCode))" -Fo Red
        wh "  0x80370114 の場合、BIOS で CPU 仮想化 (VT-x/AMD-V) を有効にしてください" -Fo Yellow
        return
    }
    $distro = Get-UbuntuDistro
    if (!$distro) { throw "ubuntu*.exe install --root の後も Ubuntu が登録されていない" }
}

# ─── Step 6: WSL 疎通確認 (このスクリプトで新たに入れたときだけ。VM を起動するので導入済みなら省く) ───
# コマンド付きの起動では OOBE は走らないので、ユーザー作成の対話で止まらない
if ($installed) {
    $test = wsl -d $distro -- echo ok 2>&1 | Out-String
    if ($test -match "ok") {
        wh "  WSL 疎通OK" -Fo Green
    } else {
        wh "  WSL 疎通失敗" -Fo Yellow
    }
}
