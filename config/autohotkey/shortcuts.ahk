; AutoHotkey Shortcuts

#Include "discord.ahk"

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
