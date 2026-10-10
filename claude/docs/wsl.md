# WSL の罠

関連: localhost のポートが Windows でも塞がる → windows-net.md / `wsl.exe -- bash -c` に `$` → bash.md

## file cache が memory 上限まで溜まり、Windows の memory を食う

- 現象: vmmemWSL が `.wslconfig` の `memory` 上限(既定 物理の半分)近くまで育つ。WSL 内の `free -m` では used は数 GB、残りは buff/cache。Windows のコミットが上限に迫り、他のアプリが落ちうる(実測 2026-10: WSL 31.9GB・コミット 87.5/88.3GB、同時刻に WezTerm が異常終了)
- `autoMemoryReclaim`(`[experimental]`、公式の既定は `dropCache`)は WSL が暇なときだけ返す。重い処理が続く間は溜まる
- 即時に返す: `wsl.exe -d <distro> -u root -- sh -c "sync; echo 1 > /proc/sys/vm/drop_caches"`。動いている処理は止めない(実測 31.9GB → 1.3GB)
- 予防: `memory` を実使用 + cache の余裕に下げる(実測: Playwright + vite preview 4 組で used 3〜5GB → 16GB で速度変わらず)。重い処理の後に上の 1 行を流す
- 反映: `.wslconfig` の変更は `wsl --shutdown` の後。動いている処理も止まるので、共有の lock を取ってから

## 暇になると止まり、/tmp が空になる

- 何も動いていない状態が約 1 分続くと止まる(`vmIdleTimeout` 既定 60000ms)。次の起動で /tmp は空
- 失敗した処理のログ・証拠を /tmp(`mktemp -d`)に置くと、調べる前に消える。作業場所の中に置く

## 設定の出典

- 設定一覧と既定値は公式 Learn の wsl-config の .md 原文で確かめる。2023 年の告知(`autoMemoryReclaim` 既定 disabled)は古い
