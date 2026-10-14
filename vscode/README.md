# vscode

VS Codeの拡張の一覧と設定を、ほかの人と共有するために置く写し。

## 自動生成のファイル

次の3ファイルは`dotcli sync`が書き出す。手で編集しない。手で直しても、次の書き出しで上書きされる。

| ファイル | 中身 |
|---|---|
| `extensions.txt` | `code --list-extensions`が出す拡張のID。小文字にそろえ、重複を除き、アルファベット順に並べ替えている |
| `settings.json` | `%APPDATA%\Code\User\settings.json`の写し。秘密の値を`<redacted>`に置き換え、コメントを除いている |
| `keybindings.json` | `%APPDATA%\Code\User\keybindings.json`の写し。`settings.json`と同じく、秘密の値を置き換え、コメントを除いている |

- 正本はVS CodeのSettings Sync。設定を変えるときはVS Codeで変える。`vscode/`のファイルはVS Codeへ書き戻さず、リンクも張らない
- `dotcli sync`は6時間に1回だけ書き出す。すぐ書き出すには`dotcli sync --force`を実行する
- 秘密の値として置き換える値は2種類ある。キー名にtoken、secret、password、auth、sessionなどを含む設定の値と、トークンの形をした文字列。判定の規則は`cli/src/commands/sync.rs`の`redact_secrets`に書いてある

`vscode/README.md`だけは手で書いている。`dotcli sync`は`vscode/README.md`を書き換えない。
