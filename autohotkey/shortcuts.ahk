; AutoHotkey Shortcuts

#Include "discord.ahk"

; .ahkファイル変更時に自動リロード
SetTimer(CheckReload, 1000)
CheckReload() {
    static lastHash := DirModHash()
    current := DirModHash()
    if (current != lastHash)
        Reload
}
DirModHash() {
    hash := ""
    loop files A_ScriptDir "\*.ahk"
        hash .= A_LoopFileTimeModified
    return hash
}

; UACダイアログ（consent.exe）を検知して自動フォーカス
; AHKが管理者権限で動作している前提（startup managerで昇格起動）
SetTimer(FocusUAC, 200)
FocusUAC() {
    ; ahk_exe では検知不可のため、タイトルとクラスで検知
    if WinExist("ユーザー アカウント制御 ahk_class Credential Dialog Xaml Host")
        WinActivate
}

; Ctrl+R をNVIDIA Broadcastより先にインターセプトしてアプリに送る
$^r::Send "{Ctrl down}r{Ctrl up}"

; WezTerm - 既に開いている場合はフォーカス、なければ新規起動
#t::{
    if WinExist("ahk_exe wezterm-gui.exe")
    {
        WinActivate
    }
    else
    {
        Run "wezterm-gui"
    }
}
