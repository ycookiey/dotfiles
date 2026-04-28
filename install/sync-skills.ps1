# sync-skills.ps1 — install/skills.json で宣言された外部skillを skills/<name>/ に取り込む
# 冪等。upstream追従は再実行。ローカル改変は上書きされる（宣言された外部skillは自前改変不可として扱う）
param([string]$Dot = (Split-Path $PSScriptRoot))

$manifest = "$Dot\install\skills.json"
if (!(Test-Path $manifest)) { return }
if (!(Get-Command gh -ea 0)) {
    Write-Host "sync-skills: gh CLI not found, skipping" -Fo Yellow
    return
}

function Sync-SkillDir([string]$Repo, [string]$Path, [string]$Ref, [string]$Dest) {
    $listJson = gh api "repos/$Repo/contents/$Path`?ref=$Ref" 2>$null
    if ($LASTEXITCODE -ne 0 -or !$listJson) {
        throw "gh api failed for $Repo/$Path@$Ref"
    }
    $entries = $listJson | ConvertFrom-Json
    New-Item -ItemType Directory -Path $Dest -Force | Out-Null
    foreach ($e in $entries) {
        $target = Join-Path $Dest $e.name
        if ($e.type -eq 'dir') {
            Sync-SkillDir $Repo $e.path $Ref $target
        } elseif ($e.type -eq 'file') {
            Invoke-WebRequest -Uri $e.download_url -OutFile $target -UseBasicParsing | Out-Null
        }
    }
}

$spec = Get-Content $manifest -Raw | ConvertFrom-Json
foreach ($s in $spec.skills) {
    $dest = "$Dot\skills\$($s.name)"
    $staging = "$dest.tmp-sync"
    try {
        if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
        Sync-SkillDir $s.repo $s.path $s.ref $staging
        if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
        Move-Item $staging $dest
        Write-Host "sync-skills: $($s.name) <- $($s.repo)/$($s.path)@$($s.ref)" -Fo Green
    } catch {
        if (Test-Path $staging) { Remove-Item $staging -Recurse -Force -ea 0 }
        Write-Host "sync-skills: failed $($s.name): $_" -Fo Red
    }
}
