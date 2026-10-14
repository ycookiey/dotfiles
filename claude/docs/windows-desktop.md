# Windows のプロセス・窓の扱い

## Explorer でファイルを選んだ状態で開く

- `explorer /select,<path>`: 一度も開いたことのないフォルダだと、フォルダは開くがファイルは選ばれない
- `SHOpenFolderAndSelectItems`(shell32): 毎回選ばれる。窓が出るまで 1〜5 秒戻らないことが多く、窓が出る前に戻ることもある → 別スレッドで呼び、窓は時間を決めて探す
- どちらも最小化や裏での表示は指定できない。窓が出たら `ShowWindow(SW_SHOWMINNOACTIVE)` で最小化する(実装例: yhooky `src/explorer.rs`)

## 裏で子プロセスを起動すると、親の出力を読む側が待たされる

- 子は親の継承可能なハンドルを全部受け継ぐ。stdout のパイプも含むので、読む側(Claude Code の hook 呼び出し等)は子が終わるまで EOF を受け取れない
- 子の stdio を null にするだけでは足りない
- 対処: spawn の間だけ `SetHandleInformation(<std handle>, HANDLE_FLAG_INHERIT, 0)` で継承を切り、終わったら戻す(例: yhooky `src/builtin/sendfile.rs` の `start_show_file`)

## 窓の手前への切り替え・入力の向き先を試す

- `LockSetForegroundWindow` は本人の入力(クリック・キー)で自動的に解ける
- 窓を開く処理(`SHOpenFolderAndSelectItems` 等)は、開いた側に手前へ出る許可を渡す。禁止が解けた瞬間に、その窓が入力を取りうる
- 裏のプロセスからの `SetForegroundWindow` は、`AttachThreadInput` でつないでも断られることが多い
- 本人が使っている PC で試すと、入力の重なりが結果を左右する。5〜10 回では方法どうしの優劣を判断できない → 試すなら本人に「15 秒ほど触らない」よう頼む

## ファイル使用中で更新・削除が失敗する

- 名前で推測して止めない。掴んでいるプロセスを特定してから止める
- 1. アプリ自身のログ: "locked by process" / "error 32" / "sharing violation" を grep
  - 例: Riot Client → `%LOCALAPPDATA%/Riot Games/Riot Client/Logs/Riot Client Logs/*.log` に「File is locked by process <名前> with PID <n> ... for path <パス>」
- 2. `handle64 <パス>` (scoop、Sysinternals): 管理者でないと見えない handle がある。対象が広いと止まることがある → 時間を区切る
- 実例: Raycast (ランチャー) が `C:/ProgramData/Riot Games/Metadata/*.db` を開いたまま → Riot の更新が失敗。Riot Client を止めても直らない
