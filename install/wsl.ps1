# install/wsl.ps1 — WSL 冪等セットアップ + Claude Code bash 切り替え
$ErrorActionPreference = "Stop"
$ProgressPreference = 'SilentlyContinue'

$GitBash = "$HOME\scoop\apps\git\current\bin\bash.exe"
$WslBash = "C:\Windows\System32\bash.exe"
$RebootFlag = "$HOME\.claude\wsl-pending-reboot"

# ─── Step 1: Claude Code bash = git-bash ───
if ((tp $GitBash) -and ([Environment]::GetEnvironmentVariable('CLAUDE_CODE_GIT_BASH_PATH', 'User') -ne $GitBash)) {
    [Environment]::SetEnvironmentVariable('CLAUDE_CODE_GIT_BASH_PATH', $GitBash, 'User')
    wh "  CLAUDE_CODE_GIT_BASH_PATH -> git-bash" -Fo Cyan
}

# ─── Step 2: Windows 機能有効化 ───
# Enable-WindowsOptionalFeature は既に有効でも State=Enabled / RestartNeeded=$false を返す（冪等）
wh "[WSL 1/4] Windows 機能チェック..." -Fo Cyan
$features = @(
    @{ Name = 'Microsoft-Windows-Subsystem-Linux'; Label = 'WSL' }
    @{ Name = 'VirtualMachinePlatform';             Label = 'VirtualMachinePlatform' }
)
$rebootNeeded = $false
foreach ($f in $features) {
    $before = (Get-WindowsOptionalFeature -Online -FeatureName $f.Name).State
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
wh "[WSL 2/4] WSL エンジン確認..." -Fo Cyan
try { $wslStatus = wsl --status 2>&1 | Out-String } catch { $wslStatus = "Error" }

if ($wslStatus -match "REGDB_E_CLASSNOTREG|エラー" -or $LASTEXITCODE -ne 0) {
    wh "  WSL エンジンをダウンロード中..." -Fo Yellow
    $msi = "$env:TEMP\wsl_core.msi"
    Invoke-WebRequest -Uri "https://github.com/microsoft/WSL/releases/download/2.3.24/wsl.2.3.24.0.x64.msi" -OutFile $msi -UseBasicParsing
    Start-Process msiexec.exe -Wait -ArgumentList "/i `"$msi`" /quiet /norestart"
    wh "  インストール完了" -Fo Green
} else {
    wh "  正常" -Fo Green
}

# ─── Step 4: Ubuntu ───
wh "[WSL 3/4] Ubuntu 確認..." -Fo Cyan
$ubuntuPkg = Get-AppxPackage *Ubuntu* | Select-Object -First 1

if (-not $ubuntuPkg) {
    wh "  Ubuntu をダウンロード中... (数分かかります)" -Fo Yellow
    $appx = "$env:TEMP\Ubuntu2204.appx"
    Invoke-WebRequest -Uri "https://aka.ms/wslubuntu2204" -OutFile $appx -UseBasicParsing
    Add-AppxPackage $appx
    $ubuntuPkg = Get-AppxPackage *Ubuntu* | Select-Object -First 1
    wh "  インストール完了" -Fo Green
} else {
    wh "  インストール済み" -Fo Green
}

# ─── Step 5: Ubuntu 初期化 ───
wh "[WSL 4/4] Ubuntu 初期化..." -Fo Cyan
wsl --set-default-version 2 2>$null | Out-Null

# wsl -l -q は UTF-16LE で出力するためヌルバイトを除去
$registered = (wsl -l -q 2>&1) -replace "`0", "" | Out-String
if ($registered -match "Ubuntu") {
    wh "  初期化済み" -Fo Green
} else {
    $exe = (Get-Command "ubuntu*.exe" -CommandType Application -ea 0 | Select-Object -First 1).Source
    if ($exe) {
        $proc = Start-Process -FilePath $exe -ArgumentList "install --root" -Wait -NoNewWindow -PassThru
        if ($proc.ExitCode -notin @(0, -1)) {
            wh "  初期化失敗 (Code: $($proc.ExitCode))" -Fo Red
            wh "  0x80370114 の場合、BIOS で CPU 仮想化 (VT-x/AMD-V) を有効にしてください" -Fo Yellow
            return
        }
    }
}

# ─── Step 6: WSL 疎通確認 ───
$test = wsl -- echo ok 2>&1 | Out-String
if ($test -match "ok") {
    wh "  WSL 疎通OK" -Fo Green
} else {
    wh "  WSL 疎通失敗" -Fo Yellow
}
