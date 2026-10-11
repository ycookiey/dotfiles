#Requires AutoHotkey v2.0

; Discordがアクティブな時のみ有効
; 挙動を変えるキーだけを定義する。同じキーを送り直すだけのホットキーは置かない
; (Discord 前面時の打鍵が LL フック経由の再送になり、何も増えないため)。
; Discord 標準のショートカット一覧は Discord 内で Ctrl + / で表示できる。
#HotIf WinActive("ahk_exe Discord.exe")

; リアクションを付ける (元: +)
+;::Send "{+}"  ; 日本語キーボードで「+」は Shift + ; の場合が多いです


; ==============================================================================
; カスタム
; ==============================================================================

; メッセージにフォーカス (入力欄からTabで抜けて↑で移動)
!m::Send "{Tab}{Up}"

; チャンネル↓ / ↑ (元: Alt+↓ / Alt+↑)
!j::Send "!{Down}"
!k::Send "!{Up}"

; ページ履歴 戻る / 進む (元: Alt+← / Alt+→)
!h::Send "!{Left}"
!l::Send "!{Right}"

; 未読チャンネル↓ / ↑ (元: Alt+Shift+↓ / Alt+Shift+↑)
!+j::Send "!+{Down}"
!+k::Send "!+{Up}"

; メンバーリスト切替 (元: Ctrl+U → VSCode風 Ctrl+B)
^b::Send "^u"

; Quick Switcher (元: Ctrl+K → VSCode風 Ctrl+P)
^p::Send "^k"

; ピン留めポップアウト (元: Ctrl+P → Ctrl+Alt+P に退避)
^!p::Send "^p"

#HotIf
