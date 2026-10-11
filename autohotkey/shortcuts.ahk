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

; F5 を NVIDIA Broadcast より先にインターセプトしてアプリに送る (nushell run-task 等)
$F5::Send "{F5}"

; WezTerm - 既に開いている場合はフォーカス、なければ新規起動
#t::{
    if WinExist("ahk_exe wezterm-gui.exe")
    {
        WinActivate
    }
    else
    {
        RunUnelevated("wezterm-gui")
    }
}

; 昇格した AHK の Run は管理者トークンと優先度を子に引き継ぐため、デスクトップの
; explorer に ShellExecute させて Medium IL・通常優先度で起動する (AHK v2 docs の ShellRun)。
; 失敗時に Run へ戻すと端末が黙って管理者になるので、戻さずに通知する。
RunUnelevated(target, args := "", dir := A_WorkingDir) {
    if !A_IsAdmin {
        Run(args = "" ? target : target " " args, dir)
        return
    }
    try {
        ShellRun(target, args, dir)
    } catch Error as e {
        TrayTip("非昇格で起動できない: " target "`n" e.Message, "AutoHotkey", "Iconx")
    }
}

ShellRun(file, args := "", dir := "", operation := "", show := 1) {
    ; SWC_DESKTOP = 8, SWFO_NEEDDISPATCH = 1。explorer が無いと例外になる
    desktop := ComObject("Shell.Application").Windows.FindWindowSW(0, 0, 8, 0, 1)
    tlb := ComObjQuery(desktop
        , "{4C96BE40-915C-11CF-99D3-00AA004AE837}"   ; SID_STopLevelBrowser
        , "{000214E2-0000-0000-C000-000000000046}")  ; IID_IShellBrowser
    ; IShellBrowser.QueryActiveShellView -> IShellView
    ComCall(15, tlb, "ptr*", sv := ComValue(13, 0))  ; VT_UNKNOWN
    ; IID_IDispatch
    NumPut("int64", 0x20400, "int64", 0x46000000000000C0, iidDispatch := Buffer(16))
    ; IShellView.GetItemObject(SVGIO_BACKGROUND) -> IShellFolderViewDual
    ComCall(15, sv, "uint", 0, "ptr", iidDispatch, "ptr*", sfvd := ComValue(9, 0))  ; VT_DISPATCH
    ; IShellDispatch2.ShellExecute
    sfvd.Application.ShellExecute(file, args, dir, operation, show)
}
