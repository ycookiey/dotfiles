# Playwright

## npm で入れた版と取得済みブラウザの revision が合わず起動しない

- 現象: `npm i playwright@latest` 直後に `browserType.launch: Executable doesn't exist at ...\ms-playwright\chromium_headless_shell-<rev>\...`
- 原因: Playwright の版ごとに使うブラウザの revision が固定。`%LOCALAPPDATA%\ms-playwright\` に古い revision しか無い
- 対処 (どちらか):
  - `npx playwright install chromium` で新 revision を取得
  - 取得済みの Chromium を直接指定: `chromium.launch({ executablePath: '<LOCALAPPDATA>/ms-playwright/chromium-<rev>/chrome-win64/chrome.exe' })`
- `channel: 'chrome'` はシステムの Chrome が標準パス (`%LOCALAPPDATA%\Google\Chrome\Application\`) に無いと失敗する (Scoop 等で入れた Chrome は見つからない)

## スマホ向けページは WebKit(iPhone の Safari のエンジン)でも撮る

- Chromium だけで効く CSS がある: `word-break: auto-phrase`(文節で折り返す)は Playwright の WebKit 26.6 で効かず、語の途中で折れた → `Intl.Segmenter` で区切り `<wbr>` を入れる
- WebKit の `fullPage` 撮影は、`loading="lazy"` の画像を読み込まない(画面外が空になる)→ 撮る前に下までスクロールし `networkidle` を待つ
- WebKit の取得は `pnpm exec playwright install webkit`(Playwright の版ごとの revision。古い版が残っていても使えない)
- 端末の設定は `devices["iPhone 13"]` 等を `newContext` に渡す

## `evaluate` / `content()` にはタイムアウトが無い: 1 ページで全体が止まる

- 現象: 多数の外部サイトを順に巡回するスクリプトが、特定ページで無期限に停止 (ログ・出力が 15 分以上進まない)
- 原因: `page.goto` / `waitForLoadState` / `screenshot` は timeout を持つが、`frame.evaluate()` と `frame.content()` は持たない。ページ内スクリプトが main thread を占有すると返らない
- 対処: 1 件分の処理全体を `Promise.race([work(), sleep(N).then(() => { throw new Error('TIMEOUT') })])` で囲み、`page.close()` も `Promise.race([page.close(), sleep(5000)])` にする
- 巡回系は「1 件ごとに結果を追記 + 完了済み id を読み込んで skip」にしておくと、止めて再開するだけで済む

## `aria-disabled="true"` のボタンは `click()` が押さない

- 現象: `click()` が `aria-disabled` を「not enabled」とみなし、押せるまで待って時間切れ(30 秒)
- 押すと検証して止める作りのボタン(未入力なら最初の欄へ移す「次へ」等)は `click({ force: true })`。押せない作りのボタンには使わない
- jsdom(testing-library)では、同じクリック中に DOM が入れ替わる不具合(段を進めた button が `type="submit"` に変わり、そのクリックで form が送信される等)が再現しない → Playwright か実ブラウザで確かめる
