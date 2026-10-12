# React

## React Compiler lint: ref を含むオブジェクトを props に渡すとエラー

- 症状: eslint-plugin-react-hooks の `react-hooks/refs` が `Cannot access refs during render` を JSX ごとに出す
- 原因: custom hook の戻り値など、`useRef` の ref を含むオブジェクトを丸ごと子の props に渡した(`<Child editor={editor} />`)。子が `.current` を読むかを lint は区別できない
- 対策: 状態・操作・ref を別々の props に分け、ref は ref 用の props(`ref={ref}` / `headingRef={ref}`)で渡す。ref を含まないオブジェクト(状態・操作)は丸ごと渡しても通る
- 実測: 管理画面の詳細で 12 件(2026-09-30)

## 'use client' の部品でもレンダー中の処理はサーバーでも走る (Next.js App Router)

- 症状: `ReferenceError: ImageData is not defined` (dev サーバーのログに `⨯`)。`useMemo` の中で `new ImageData(...)` / `document.createElement('canvas')` を作っていた
- 原因: クライアント部品も初回はサーバーで HTML を作るためにレンダーされ、`useState` 初期化・`useMemo`・本体のコードはサーバーで実行される。`useEffect` の中だけがブラウザ専用
- 対処: レンダー中は純粋なデータ (配列など) だけを作り、`ImageData`・canvas・`Image` などブラウザ API は `useEffect` (描画処理) の中で作る。作り直しを避けたいときは ref にキャッシュする
- 確認: `tsc` と lint は通ってしまう。ページを curl で取得して dev サーバーのログに `⨯` が出ないことを見る

## rAF の再生ループで経過時間が負になる (rAF の時刻は frame の開始時刻)

- 症状: 再生直後に `Cannot read properties of null`。t=0 から再生すると時刻が 0 未満になり、記録の範囲外を引く
- 原因: effect で `last = performance.now()`、rAF の引数 `now` との差を経過時間にした。`now` は frame の開始時刻で、最初の 1 回は `last` より前になりうる
- 対処: `dt = Math.max(0, now - last)` (または最初の callback は `last = now` だけ)
- 再現: headless では時機しだいで出ない。`addInitScript` で `requestAnimationFrame = cb => raf(() => cb(performance.now() - 100))` に差し替えると毎回出る
