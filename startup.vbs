' Windows スタートアップ用スクリプト

Set objShell = CreateObject("WScript.Shell")
Set objFSO = CreateObject("Scripting.FileSystemObject")

scriptDir = objFSO.GetParentFolderName(WScript.ScriptFullName)
' === AutoHotkey の起動 ===
ahkScriptPath = scriptDir & "\config\autohotkey\shortcuts.ahk"

If objFSO.FileExists(ahkScriptPath) Then
    ' AutoHotkey を起動（0: ウィンドウを非表示, False: 完了を待たない）
    objShell.Run """AutoHotkey.exe"" """ & ahkScriptPath & """", 0, False
End If