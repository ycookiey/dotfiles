# Vivaldi 検索エンジン設定の適用
# default_search_provider の GUID を書き換えてデフォルトエンジンを変更する

$ScriptDir = Split-Path $MyInvocation.MyCommand.Definition
. "$ScriptDir\..\pwsh\aliases.ps1"

$profileDir = "$HOME\scoop\apps\vivaldi\current\User Data\Default"
$prefs = "$profileDir\Preferences"

if (!(tp $prefs)) {
    throw "Vivaldi Preferences が見つからない: $prefs"
}

if (Get-Process vivaldi -ea 0) {
    wh "Vivaldi が起動中のためスキップ" -Fo Yellow
    return
}

$webData = "$profileDir\Web Data"
if (!(tp $webData)) {
    throw "Web Data が見つからない: $webData"
}

# Web Data から Google の sync_guid を取得 (prepopulate_id=1)
$guid = sqlite3 $webData "SELECT sync_guid FROM keywords WHERE prepopulate_id = 1 LIMIT 1;"
if (!$guid) {
    throw "Google の検索エンジンエントリが見つからない"
}

$json = gc $prefs -Raw | ConvertFrom-Json

# デフォルト検索エンジン: Google
$json.default_search_provider.guid = $guid
$json.default_search_provider.guid_search_field = $guid
$json.default_search_provider.guid_speeddials = $guid

$json | ConvertTo-Json -Depth 100 -Compress > $prefs
wh "デフォルト検索エンジンを Google に設定した"
