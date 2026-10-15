# ブラウザ拡張機能(Chromium 系・MV3)の挙動

Web サイトと拡張機能を組み合わせる作り(サイトが拡張機能経由でブラウザの API を読む)で実測した挙動。確認環境: Vivaldi 8.2(Chromium 系)、2026-10。

## 権限

- `chrome.sessions` が返す tab の `url`・`title`・`favIconUrl` は、`tabs` 権限(またはその page への host 権限)がないと欠ける。行は返るので、空の一覧に見えて気づきにくい。`tabs.Tab` の各 property の注記に書いてある
- 権限の後足し = manifest 変更 = 入れている全 PC で拡張機能の再読み込み。使う API の返り値が権限で欠けないかを、公式 reference の各 property の注記で先に確かめる

## 置き換えと再読み込み

- Vivaldi は `chrome_url_overrides.history` を受け付けない(読み込みは成功、履歴の page は置き換わらない)。Chrome は可。Vivaldi で Ctrl+H を変えるには、設定のクイックコマンドでコマンドチェインを作り(コマンド「新しいタブでリンクを開く」に URL)、設定のキーボードで割り当てる
- パッケージ化されていない拡張機能: 拡張機能の page(HTML/JS/CSS)は tab の F5 で反映。`manifest.json` と service worker の変更は拡張機能の再読み込みが必要

## Web page とのやり取り

- `externally_connectable.matches` はポートまで書ける。`http://localhost:8765/*` で、他ポートと `127.0.0.1` は拒否された
- 許可外の origin では `chrome.runtime` 自体が無い。許可された origin で拡張機能が無効のときは `sendMessage` が存在し、数十 ms で `Could not establish connection. Receiving end does not exist.` が返る → どちらも即座に未導入と判定できる
- `_favicon/*` を `web_accessible_resources`(matches をサイトに限定)に入れると、Web page の `<img>` から `chrome-extension://<id>/_favicon/?pageUrl=<URL>&size=32` を読める。サイトの CSP `img-src` に `chrome-extension://<id>` が必要。公式の例は content script 経由だけだが、page から直接読めた
- サイトの CSP `frame-ancestors` は `chrome-extension:`・`chrome-extension://<id>` のどちらでも、拡張機能の page からの iframe 埋め込みを許可した。`'none'` で拒否
- 拡張機能の ID は manifest の `key`(公開鍵 DER の base64)で固定できる。ID = 公開鍵の SHA-256 の先頭 32 桁の 16 進数を 0-f → a-p に置き換えたもの。パッケージ化されていない拡張機能は `key` がないと置き場のフォルダで ID が変わる

## Vivaldi の画面の表記を確かめる

- `<Vivaldi>/Application/<版>/resources/vivaldi/_locales/ja/messages.json`: Vivaldi 独自の文言。キーは英語の原文の英小文字・数字以外の文字を `_<文字コード>_` に置き換え、末尾に `0`(例: `Command Chains` → `_67_ommand_32__67_hains0`)。JSON として読めないことがあるので、キーを作って文字列で探す
- `<Vivaldi>/Application/<版>/Locales/ja.pak`: Chromium 由来の文言。UTF-8 のまま入っていて、文字列で探せる
- 例: 「コマンドチェイン」(チェーンではない)・「新しいタブでリンクを開く」・「非パッケージ拡張機能を読み込む」(Chrome は「パッケージ化されていない拡張機能を読み込む」)・「デベロッパー モード」(間に空白)
- scoop で入れた、自分で更新するアプリ(Vivaldi 等)は、scoop の manifest の版と実際の版がずれる。実際の版は `Application/` の下の版のフォルダで確かめる
