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
