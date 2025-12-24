; AutoHotkey Shortcuts
; Windows Terminal - 既に開いている場合はフォーカス、なければ新規起動
#t::{
    if WinExist("ahk_exe WindowsTerminal.exe")
    {
        WinActivate
    }
    else
    {
        Run "wt"
    }
}
