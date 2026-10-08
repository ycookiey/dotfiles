# Python asyncio / 並行処理の罠

## 共有 Task を await する側が cancel されると、共有 Task も cancel される

- asyncio の仕様: task が await 中の Future/Task に cancel が伝播する
- 複数の呼び出し元で共有する処理(先行起動・バックグラウンド import・一度だけの初期化等)は `asyncio.shield(task)` を返して await させる。呼び出し元の1つが cancel されても共有処理は継続
- `CancelledError` は `Exception` の子でない(`BaseException` の子)。`except Exception` で拾えず、worker が例外ログも出さず黙って終わる

## Textual: `run_worker(exclusive=True)` は同じ group の worker を全部 cancel

- group 未指定は全て `"default"`。exclusive な worker を起動すると、無関係な先行処理の worker も cancel される
- 先行処理の worker は `run_worker(..., group="prestart")` のように group を分ける
- 実例: モード選択画面で先行起動を始め、ユーザーが即選択 → Bot 起動 worker(exclusive)が先行起動 worker を cancel → 共有の生成 task も cancel → Bot 起動 worker が CancelledError で黙って終了し「Starting...」のまま止まる
- テスト: 共有処理に遅延を入れ、`App.run_test()` の pilot で即座にキー入力して到達を assert(共有処理の完了を先に待つテストでは再現しない)

## 重い import のスレッドでの先読みは効かない (GIL)

- import は CPU 処理 (bytecode 読込・module 実行) が主体。別スレッドで先読みしても GIL の取り合いで並列化されない
- 実測 (Textual TUI + discord.py、各10回の中央値): 先読みなし 画面 0.76秒 / discord 利用可 1.47秒、先読みあり 1.0秒 / 1.67秒。改善なし、画面表示が遅れる傾向
- 2スレッドが同じモジュールを同時に import → 循環 import の場合、デッドロックの代わりに初期化途中のモジュールを返す (importlib の `_DeadlockError` 回避)。まれに AttributeError
- 効果が見込めるのは I/O 待ちが主体の場合 (キャッシュが冷えたディスク等) のみ。判断は実アプリ相当の起動で複数回測ってから (単純な import 比較では差が出て見えた)
