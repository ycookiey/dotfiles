# install/wsl.ps1 — WSL 冪等セットアップ + Claude Code bash 切り替え
$ErrorActionPreference = "Stop"
$ProgressPreference = 'SilentlyContinue'

$GitBash = "$HOME\scoop\apps\git\current\bin\bash.exe"
$WslBash = "C:\Windows\System32\bash.exe"
$RebootFlag = "$HOME\.claude\wsl-pending-reboot"

# ─── Step 1: git-bash フォールバック ───
$currentBash = [Environment]::GetEnvironmentVariable('CLAUDE_CODE_GIT_BASH_PATH', 'User')
if (!$currentBash -and (tp $GitBash)) {
    [Environment]::SetEnvironmentVariable('CLAUDE_CODE_GIT_BASH_PATH', $GitBash, 'User')
    wh "  CLAUDE_CODE_GIT_BASH_PATH -> git-bash" -Fo Cyan
}

# ─── Step 2: Windows 機能有効化 ───
wh "[WSL 1/4] Windows 機能チェック..." -Fo Cyan
$wslF = Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Windows-Subsystem-Linux -All -NoRestart
$vmpF = Enable-WindowsOptionalFeature -Online -FeatureName VirtualMachinePlatform -All -NoRestart

if ($wslF.RestartNeeded -or $vmpF.RestartNeeded) {
    "pending" | sc $RebootFlag
    wh "  機能を有効化しました。再起動後に setup.ps1 を再実行してください。" -Fo Yellow
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

# ─── Step 6: 疎通確認 → WSL bash に切り替え ───
$test = wsl -- echo ok 2>&1 | Out-String
if ($test -match "ok") {
    [Environment]::SetEnvironmentVariable('CLAUDE_CODE_GIT_BASH_PATH', $WslBash, 'User')
    wh "  CLAUDE_CODE_GIT_BASH_PATH -> WSL bash に切り替えました" -Fo Green
} else {
    wh "  WSL 疎通失敗。git-bash を維持します。" -Fo Yellow
}
