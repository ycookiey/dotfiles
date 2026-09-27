# Claude in Chrome: hidden タブ・stale ref・blur 不発・非 ASCII 入力の落とし穴

ブラウザ自動化で送信ボタンが反応しないとき、原因は 2 つある。タブが hidden であるか、read_page で取った ref が古いか。前者はユーザーにタブを 1 度切り替えてもらうしかなく、後者は貼り付け後に ref を取り直せば直る。

## 症状 1: hidden タブでは送信系の操作が発火しない

`document.hidden === true` の状態のタブに対して以下を実行しても、送信・投稿系のハンドラが発火しない:

- `mcp__claude-in-chrome__computer` の `left_click`（ref 指定・座標指定とも）
- `javascript_tool` で `element.click()` や `dispatchEvent(new MouseEvent('click', ...))`
- `pointerdown`, `mousedown`, `pointerup`, `mouseup`, `click` の連続 dispatch

一方、hidden 状態でも動く操作:

- `contenteditable` への Ctrl+V による貼り付け
- 読み取り系（`read_page`、JavaScript での DOM 取得）
- 送信を伴わないダイアログ開閉

### 原因

Chrome の user activation ポリシー。合成イベントは `isTrusted: false` で user gesture としてカウントされない。CDP 経由の click でも、hidden タブでは Angular Material などのフレームワーク側がハンドラを抑制する。

### 効かなかった回避策（試行済み）

- 同じ URL への `navigate` → タブがすでにアクティブなら visible を保つが、**別タブがアクティブな状態では visible にならない**
- `tabs_create_mcp` での新規タブ作成 → 新タブも hidden のまま
- PowerShell の `SetForegroundWindow`（`AttachThreadInput` や Alt キー押下でフォアグラウンドロックを回避しても）→ ウィンドウは前面化できるが、**ブラウザのアクティブタブは変わらない**
- `keybd_event` での Ctrl+1 / Ctrl+9 / Ctrl+Tab 送信 → タブ切り替えが起きない
- `SendKeys` → 同様に届かない
- `resize_window` → 効果なし

### 唯一有効な回避策

**ユーザーに対象タブを 1 度クリックしてもらう。** 以降は同じタブ内で `navigate` し続ける限りアクティブ状態が維持されるので、複数回の送信を自動で回せる。

ブラウザウィンドウ全体が背景に落ちても hidden になるため、マルチモニタ環境ならブラウザを別モニタに置いて常時 visible を保つと安定する（PowerShell の `MoveWindow` でウィンドウ移動は自動化できる）。

## 症状 2: read_page の ref は DOM 変化で無効になる

貼り付け前に取得した ref を貼り付け後にクリックしても、**エラーは出ないのに何も起きない**。

```
read_page → ref_30 = 送信ボタン
Ctrl+V で 20 万文字を貼り付け（Angular が DOM を再構築）
computer left_click ref_30 → "Clicked on element ref_30" と返るが送信されない
```

`read_page` を取り直すと同じボタンが `ref_31` や `ref_35` に変わっており、新しい ref でクリックすると送信できる。

### 鉄則

**DOM を変える操作（貼り付け、ナビゲーション、モーダル開閉）のたびに `read_page` を取り直す。** ref は取得時点のスナップショットで、古い ref のクリックは成功したように見えて無反応になる。

長文の貼り付けは特に DOM 変化が大きい。「入力欄にテキストは入っているのに送信できない」ときは、まず ref の鮮度を疑う。

## 症状 3: hidden タブでは実キーでも focus / blur が起きない

- 別タブが前面だと `visibilityState: "hidden"`。`computer` の `key` で Tab を送ってもフォーカスが移らず、onBlur 系(フォーム検証、blur 時の API 呼び出し)が発火しない
- 検証目的なら `el.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))` で代替できる(React の onBlur は focusout を拾う)。user activation が要る送信系には効かない(症状 1)

## 症状 4: `computer` の `type` で非 ASCII が入らない

- カタカナ等を type しても値が変わらない(ASCII は入る)
- 値は native setter + input イベントで入れる:

```js
Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, v)
el.dispatchEvent(new Event('input', { bubbles: true }))
```

## 検証手順

タブの可視状態:

```js
({ hidden: document.hidden, focus: document.hasFocus(), visibility: document.visibilityState })
```

送信の成否は「入力欄がクリアされたか」「URL にチャット ID が付いたか」「メッセージ要素が増えたか」で判定する:

```js
(() => {
  const rte = document.querySelector('[contenteditable="true"]');
  return {
    input_len: rte ? rte.textContent.length : null,
    msg_count: document.querySelectorAll('user-query, message-content, model-response').length,
    url: location.href
  };
})()
```

## 結果の回収は DOM から直接取る

コピーボタンを押してクリップボード経由で取る方法より、DOM から直接取る方が確実で速い:

```js
// Claude
Array.from(document.querySelectorAll('.font-claude-response ol li')).map(li => li.textContent.trim()).join('\n')

// Gemini
Array.from(document.querySelectorAll('model-response ol li')).map(li => li.textContent.trim()).join('\n')
```

応答の完了判定は停止ボタンの有無で行う:

```js
(() => {
  const stop = Array.from(document.querySelectorAll('button')).find(b => (b.getAttribute('aria-label')||'').match(/停止|回答を停止|Stop/));
  const ol = document.querySelector('model-response ol, .font-claude-response ol');
  return {generating: !!stop, items: ol ? ol.querySelectorAll('li').length : 0};
})()
```

## その他の注意

- **クリップボードは他の操作で上書きされる。** 貼り付け直前にクリップボードへ載せ、貼り付け後は `input_len` で長さを検証する。想定と違う長さなら別の内容が入っている
- **`computer` の `wait` は最大 10 秒。** それ以上待つ場合は複数回に分ける
- **Chrome 拡張は長時間の操作で切断されることがある。** `tabs_context_mcp` がタイムアウトを返し始めたら、ブラウザ側の再接続が必要
- **Git Bash の `/tmp` は Windows の Python から見えない。** ファイル受け渡しは Windows パスに統一するか、同じシェル内で完結させる
