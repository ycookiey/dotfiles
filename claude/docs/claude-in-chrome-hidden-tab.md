# Claude in Chrome: hidden タブ・stale ref・blur 不発・非 ASCII 入力・遅延読み込み停止・大量データ回収の落とし穴

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

## 症状 5: hidden タブでは遅延読み込み・タイマーが止まる

- スクロールで古い履歴を読み込む画面(チャット等)は、hidden タブではスクロールしても読み込まれない(IntersectionObserver / rAF 系が動かない)
- `setTimeout` は間引かれる。状態だけ変わり DOM が変わらない更新では MutationObserver が発火しない → 待ちがタイムアウトまで延びる
- `javascript_tool` の中で `requestAnimationFrame` を待つと返らず、CDP が 45 秒で打ち切る (`Runtime.evaluate timed out ... renderer may be frozen`)。固まったのではない。canvas の再生・アニメーションの速さ (fps・1 フレームの時間) も hidden では測れない。先に `document.visibilityState` を見て、hidden なら速さは処理側 (Node の計測 script や console.log の処理時間) で測る
- 待機: `MessageChannel` の 1 タスクごとに条件をポーリング(postMessage は間引かれない)

```js
const tick = () => new Promise((r) => { const c = new MessageChannel(); c.port1.onmessage = () => r(); c.port2.postMessage(null) })
```

- 移動: SPA は `history.pushState({}, '', url)` + `dispatchEvent(new PopStateEvent('popstate', { state: {} }))` で再読み込みなし。URL で位置を指定できる画面(例: Discord のメッセージ ID)は、スクロールの代わりに URL 移動で読み込ませる
- 構造化データ: 要素の `__reactFiber…` キーの `memoizedProps` を `.return` で親へたどると、画面が持つデータ(本文・日時・添付等)が取れる。

## 症状 6: ブラウザ窓が見えていないと screenshot が timeout する

- `computer` の `screenshot` が `Page.captureScreenshot timed out after 30000ms ... renderer may be frozen` を返す。`javascript_tool` は普通に返るので固まってはいない(窓が最小化・別仮想デスクトップ等で描画されていない)
- 画像を `javascript_tool` 経由で取る案(canvas の `toDataURL` を返す)は、戻り値が `[BLOCKED: Base64 encoded data]` になり使えない
- 自分の開発中ページ(localhost)の見た目確認なら、取得済みの Playwright 用 Chromium(`%LOCALAPPDATA%\ms-playwright\chromium-<rev>\chrome-win64\chrome.exe`)を `--headless=new --remote-debugging-port=<port> --user-data-dir=<tmp>` で起動し、Node 標準の `WebSocket` で CDP を直接叩く。npm package 不要
  - `GET http://127.0.0.1:<port>/json` → page の `webSocketDebuggerUrl` に接続 → `Page.navigate` → `Runtime.evaluate`(`awaitPromise: true`)でボタン操作 → `Page.captureScreenshot` に `clip` と `captureBeyondViewport: true` を渡して要素だけ PNG 保存 → Read で見る
  - 要素の位置は `getBoundingClientRect()` + `scrollX/scrollY`(clip は document 座標)
  - 終了は CDP の `Browser.close` で行う(`/json/version` の `webSocketDebuggerUrl` に接続して送る)。Node の `proc.kill()` は親だけを止め、子プロセス(renderer・crashpad 等)が残る。`taskkill /PID <pid> /T /F` を exit 時に呼んでも残った。実測: 撮影を繰り返して 74 個溜まり、同じ port で次の起動が接続できなくなった
  - 残りの確認と掃除(PowerShell): `Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" | ? { $_.CommandLine -like '*ms-playwright*' }`。ユーザーの通常の Chrome を巻き込まないよう、コマンドラインで絞ってから止める

## 大量データの回収

- `javascript_tool` の戻り値は約 1,000〜1,500 字で `[TRUNCATED]`
- 大きいデータ: ページに `<article>`(表示状態)を足して文字列を入れ → `get_page_text` で読む(12K 字超でも切れなかった)。読んだら要素を消す
- `get_page_text` は空白を詰める(全角スペース・NBSP も半角 1 つ、連続空白も 1 つ) → 空白は目印(例 `⟦u3000⟧`)に置き換えて渡し、受け側で戻す。受け取った文字列をツール入力に書き写すとバックスラッシュのエスケープが化ける(bash.md)ので、バックスラッシュも目印にする
- ファイルに残すだけなら、ページから手元の HTTP server(127.0.0.1)へ POST する(モデルを通らない。28 件のチャンネルで書き出し 9.2k 字 → 110 字、2026-10-11)
  - https のページから http://127.0.0.1 への fetch は Chrome で通る。サイトの CSP(connect-src)次第。読み込んだ文字列の eval も同じく CSP 次第
  - 受け側は Origin を確かめ、起動ごとの合言葉を path に入れて、ほかのサイトのページや別のプロセスからの書き込みを拒む(127.0.0.1 へは単純な POST がどのサイトからも届く)
  - 起動の一文だけをページに渡し、手順本体は server から読み込ませると、スクリプトを貼る出力も減る

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

- **clipboard は他 process に上書きされる** → paste 直前に set。paste 後は `input_len` と先頭文字列で検証
- **`textContent` は改行を含まない**(Quill 等の contenteditable は段落を要素に分ける) → 完全性は「全文から改行を除いた字数」と比べる。生の字数と比べると切れたと誤判定する(実例: 改行込み 149,832 字の入力が 140,657 字 = 改行除去後と完全一致)
- **`computer` の `wait` は最大 10 秒。** それ以上待つ場合は複数回に分ける
- **Chrome 拡張は長時間の操作で切断されることがある。** `tabs_context_mcp` がタイムアウトを返し始めたら、ブラウザ側の再接続が必要

## LLM Web UI で同じ prompt を反復実行する(実験用)

- 送信前に model 名を JS で照合、不一致なら中止: 既定 model は予告なく変わる(claude.ai が Opus 5.5 既定に)。Gemini は Pro 上限で Flash 系へ自動 fallback
- 上限時は model menu に「上限は M月D日 H:MMにリセット」 → reset 時刻に CronCreate(one-shot)で自分への再開リマインドを予約
- 長文 input は file 添付: Gemini「アップロードとツール」click → `find` で file input の ref → `file_upload`(session で共有済みの path のみ)。本文は指示文だけ貼る
- `browser_batch` は合計待機が約 60 秒を超えると timeout(中の action は実行継続) → 送信までを batch、生成待ちは `wait`(≤10 秒)を単発で
- 結果が長いときは上の「大量データの回収」(`<article>` + `get_page_text`)で一度に取る
- Gemini で file 添付時、`li.textContent` 末尾に添付 chip の「TXT」が混入 → `.replace(/\s+TXT$/, '')`
- 出力に英語表記の併記が混じる(`実装データ型 (ImplementationDataType)`) → 照合時に末尾の括弧を除去
- 1 応答に下書き + 最終版の複数リスト(`ol` が 2〜3 個、間に「最終版を書く」等)、同じ 100 件が 2 回続いて 1 つの `ol` に 200 件 → 最後のリスト / 後半を採用
- 件数不足・崩れ: Opus 4.8 が 87 件で止めて「残りも埋めるか」と問い返す、Gemini Flash が途中で先頭に戻り同じ語を繰り返す(145 件) → 保存前に件数と重複なし件数を照合、崩れた出力は別 folder に残して同条件で出し直し、回数を記録
- 大きい paste(Gemini 15 万字超)は paste 自体が重く、paste + 送信 + 待機を 1 batch にすると timeout → paste と送信確認を別 batch に分ける
- 結果をコピーボタン経由(クリップボード)で保存する場合: 押し損じで中身が別物になる → 画面の DOM から数えた語数・文字数と照合、0 件なら保存しない。tool の返り値経由(書き写し)より転記ミスがない
- clipboard を使う自動化はユーザーの作業と衝突(ユーザーのコピーが自分の paste に入る / 自分がユーザーのクリップボードを上書き) → paste 直後に貼り付け行数(claude.ai は添付 chip の「N行」)と入力欄が空かを照合してから送信。実行前にユーザーへ告知

## 複数タブで並列実行(Vivaldi のタイル表示)

- 生成待ち(1 回 40〜90 秒)が律速 → 3 タブ同時生成で 1 回あたり約 1.6 分 → 0.6 分(実測、claude.ai 27 回)
- `tabs_create_mcp` は MCP タブグループがあるウィンドウにしか作れない(最後にフォーカスしたウィンドウではない)。`vivaldi.exe --new-window` で開いた窓、ユーザーがクリックした窓でも不可 → 別ウィンドウ案は使えない
- タイル表示(グループ内のタブを Ctrl+クリックで複数選択 → 右クリック →タイル表示)なら全タブ `visibilityState: "visible"`。タブはグループに残り操作可能
- キー入力はフォーカス中のタイルにしか届かない。JS の `el.focus()` では移らない → `computer` で入力欄を実クリックしてから Ctrl+V
- タイルごとに viewport が違い、スクリーンショット座標 = CSS px × (画像幅 / innerWidth)。細いタイルでは換算座標のクリックが外れた → `find` で ref を取ってクリック
- クリップボードは 1 つ → paste・保存(コピーボタン)は 1 タブずつ順番、生成だけを並行。送信 batch に他タブの完了確認 JS を同梱すると往復が減る
- tool 呼び出しを 1 応答で並べると並行実行されうる → clipboard を読み書きする手順(コピー → paste、クリック → 保存)は応答を分ける
