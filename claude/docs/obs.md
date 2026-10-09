# OBS Studio

## WebSocket で設定を変えて操作する

- 接続: `obsws-python`(PyPI)。パスワードは `%APPDATA%\obs-studio\plugin_config\obs-websocket\config.json` の `server_password`。サーバーの有効化は OBS の「ツール」→「WebSocket サーバー設定」(既定は無効)
- 簡易モードの録画品質(`SetProfileParameter` の `SimpleOutput` / `RecQuality`)を変えても、出力は作り直されない。起動時に「配信と同じ」(`Stream`)だと録画専用のエンコーダが無く、そのまま `StartRecord` すると失敗する
  - 症状: `StartRecord` は返るが録画が始まらず、`StopRecord` が 501(出力が動いていない)。ログにエンコーダの行が出ない。「録画開始に失敗しました」の窓が、メインの窓の裏に開く(撮っても見えない。プロセスの見える窓の一覧で見つかる)
  - 直し方: `SetCurrentProfile` で別のプロファイルに切り替えて戻す(出力の設定を読み込み直す)。「設定」の OK でも直る
  - 確かめ方: 数秒だけ録画し、OBS のログ(`%APPDATA%\obs-studio\logs\` の最新)に録画用のエンコーダの設定(例: `[obs-nvenc: 'simple_video_recording']`、`rate_control: CQP`)と `==== Recording Start` が出るか見る
- `SetVideoSettings` の解像度の変更は、その場で効く(ログに `video settings reset`)

## 設定のファイルを直接書き換える

- OBS の起動中に書き換えない(終了時に上書きされる)。`tasklist` で `obs64.exe` が無いことを確かめてから
- プロファイル: `basic\profiles\<名前>\basic.ini`。出力の解像度は `[Video]` の `OutputCX`/`OutputCY`、簡易モードの録画品質は `[SimpleOutput]` の `RecQuality`
- シーンコレクション: `basic\scenes\<名前>.json`(表示名の空白はファイル名で `_`)。表示中のシーンは `current_scene`・`current_program_scene`
- 全シーン共通の音声デバイス(「設定」→「音声」のマイク・デスクトップ音声)は `sources` の一覧に無く、`AuxAudioDevice1`〜・`DesktopAudioDevice1`〜 の項目にある。ミュートは各項目の `muted`。WebSocket では `GetSpecialInputs` で名前を引き、`SetInputMute`
