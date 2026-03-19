# bootstrap ガイド

新しいソフトのインストールやシステム設定の変更を行う際、bootstrap への追加が必要か判断するためのガイド。
リポジトリルート: dotfiles（通常 C:\Main\Project\dotfiles）

## bootstrap対象の判断基準

**追加すべきもの:**
- 新しいPC/初期化後のPCで自動的に再現したい設定やアプリ
- 他のツールの前提となる環境設定

**追加すべきでないもの:**
- ユーザー固有の認証情報・トークン
- 一時的な設定・実験的なツール
- 手動セットアップが前提のもの（OAuthログイン等）

## 実行フロー

bootstrap.ps1（リポジトリルート）は以下の順序で実行される。

### Phase 0（bootstrap.ps1 直接）: 前提条件

bootstrap.ps1 自体に直接書く。Phase 1 以降の前提となる設定。

- 管理者権限が必要なシステム設定（レジストリ等）
- Scoop/git/pwsh のインストール
- リポジトリの clone

### Phase 1: コアアプリ — install/scoop.ps1 -SkipLarge

install/scoopfile.json に記載された小型アプリをインストール。
install/install-order.json の `large` リストに含まれないものが対象。
失敗時は bootstrap 全体が中断する。

### Phase 2: 設定 — setup.ps1

管理者昇格を含む。内部で以下を実行:
- install/webinstall.ps1（webinstall.json ベース）
- シンボリックリンク作成（詳細は symlinks.md）
- ファイル関連付け（Neovim + WezTerm）
- PATH 追加（`$HOME\.local\bin`）
- install/wsl.ps1（WSL + bash設定）
- Claude マルチアカウントのリンク同期
- install/fonts.ps1
- install/winget.ps1（wingetfile.json ベース）
- dotcli ビルド＆エイリアス生成
- startup/register.ps1（タスクスケジューラ登録）

### Phase 3: dev tool ランタイム — mise install

mise が利用可能な場合のみ実行。`~/.config/mise/config.toml`（→ mise.toml）で管理。
node, python, java, awscli 等はここで入る。

### Phase 4: 大型アプリ — install/scoop.ps1 -OnlyLarge

install-order.json の `large` リストに含まれるアプリ。失敗しても bootstrap は続行。

### Phase 5: スタートアップ — startup/manager.ps1

WezTerm 等が未起動の場合のみ実行。

## 追加先の早見表

| 追加したいもの | 追加先 |
|---|---|
| Scoop アプリ | install/scoopfile.json（大型なら install-order.json の large にも追加） |
| Winget アプリ | install/wingetfile.json |
| webinstall ツール | install/webinstall.json |
| dev ランタイム（node, python等） | mise.toml |
| システム設定（レジストリ、管理者必須） | bootstrap.ps1 の Phase 0 部分 |
| 設定ファイルのシンボリックリンク | setup.ps1 + symlinks.md 参照 |
| フォント | install/fonts.ps1 |
| WSL 関連 | install/wsl.ps1 |
