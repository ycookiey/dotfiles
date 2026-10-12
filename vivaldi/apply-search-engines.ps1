# Vivaldi 検索エンジン設定の適用
# default_search_provider の GUID を書き換えてデフォルトエンジンを変更する

$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"

# sqlite3 がないと後段の GUID 取得が空になり原因と違うメッセージになるため先に確認
if (!(gcm sqlite3 -ea 0)) {
    throw "sqlite3 が見つからない (scoop install sqlite)"
}

$profileDir = "$HOME\scoop\apps\vivaldi\current\User Data\Default"
$prefs = "$profileDir\Preferences"

if (!(tp $prefs)) {
    throw "Vivaldi Preferences が見つからない: $prefs"
}

# 起動中は Preferences が終了時に上書きされるため適用しない。
# throw で呼び出し側 (setup.ps1 の catch) に未適用を伝える
if (Get-Process vivaldi -ea 0) {
    throw "Vivaldi が起動中のため未適用 (終了してから再実行)"
}

$webData = "$profileDir\Web Data"
if (!(tp $webData)) {
    throw "Web Data が見つからない: $webData"
}

# Web Data から Google の sync_guid を取得 (prepopulate_id=1)
$guid = sqlite3 $webData "SELECT sync_guid FROM keywords WHERE prepopulate_id = 1 LIMIT 1;"
if ($LASTEXITCODE -ne 0) {
    throw "sqlite3 が失敗 (exit $LASTEXITCODE): $webData"
}
if (!$guid) {
    throw "Google の検索エンジンエントリが見つからない"
}

$json = gc $prefs -Raw | ConvertFrom-Json
$dsp = $json.default_search_provider

# 既に目的の値なら書き込まない
if ($dsp.guid -eq $guid -and $dsp.guid_search_field -eq $guid -and $dsp.guid_speeddials -eq $guid) {
    wh "デフォルト検索エンジンは既に Google"
    return
}

# デフォルト検索エンジン: Google
$json.default_search_provider.guid = $guid
$json.default_search_provider.guid_search_field = $guid
$json.default_search_provider.guid_speeddials = $guid

$json | ConvertTo-Json -Depth 100 -Compress > $prefs
wh "デフォルト検索エンジンを Google に設定した"
