# ywatchy

ファイル監視で symlink/sync を自動管理する Rust 製ツール。scoop shim 経由で常駐。実体は `C:\Main\Project\ywatchy`。

## 役割

- **skill 個別 symlink 自動管理**: 走査対象プロジェクト配下に skill ディレクトリを作ると `~/.claude/skills/<name>` への symlink を自動生成 (setup.ps1 への追加不要)。dotfiles 限定ではない
- **project store sync**: 各プロジェクトと `~/.ywatchy/store` を双方向 sync

setup.ps1 管理外。skill ディレクトリだけ作れば残りは ywatchy 担当。

## 探索パターン

既定 (`ywatchy.toml` の `skills.skill_patterns`) は 2 つ。`skills.scan_dirs` に指定したディレクトリ配下の各プロジェクトを走査する。

- `skills/*/SKILL.md` — dotfiles 等、リポジトリ直下に skills を置く構成
- `.claude/skills/*/SKILL.md` — プロジェクト固有 skill

つまり **dotfiles 以外のプロジェクトの skill も自動で symlink される**。そのプロジェクトの中でしか使わない skill は、`skills.exclude_projects` にプロジェクト名(フォルダ名、大文字小文字は区別しない)を書くと global に張られない。既存の symlink は次の起動時に消える。プロジェクト内では `.claude/skills/` の skill としてそのまま読まれる(v0.4.0 以降)。

## skill 追加手順

1. `<project>/skills/<name>/SKILL.md` または `<project>/.claude/skills/<name>/SKILL.md` 作成 (frontmatter `name`/`description` 必須)
2. 追加リソースを同ディレクトリに配置 (`*.sh`, `*.py` 等)
3. **以上**。setup.ps1 編集不要。ywatchy が `~/.claude/skills/<name>` symlink を張る

確認: `readlink ~/.claude/skills/<name>` で実体パスが出れば成功。

## 前提: scan_dirs と常駐

`ywatchy.toml` の `skills.scan_dirs` が空だと何も走査しない。自動化を効かせるには走査ルート (例: `C:/Main/Project`) を設定し、ywatchy を常駐させる。

**このマシン**: 設定は `%APPDATA%\ywatchy\ywatchy.toml`。`scan_dirs = ["C:/Main/Project"]` 設定済み。自動起動 (`dotfiles/startup/manager.ps1`) には無く、PC 再起動後は止まっている。新しい skill を自動で張るには ywatchy を起動して常駐させる (例: `Start-Process (Get-Command ywatchy).Source -WindowStyle Hidden`)。

起動時に走るもの:
- skill の symlink 作成 (既存の実体ディレクトリと同名なら変更せずエラー 1 行)
- CLAUDE.md の初回同期。更新時刻の新しい側が勝つ (store 側が新しければ source を上書き)
- 走査先を指すのに見つからない skill の symlink 削除

起動前に、消える symlink と store→source の上書きが無いかを照合する。

## 状態確認

```
ywatchy        # 起動するとリアルタイムで sync 状態を表示
```

凡例:
- `✓` 同期済
- `★ source -> store` source 側に変更あり、store へ反映中
- `+ source -> store` store 未登録、新規追加中

## 配布

scoop bucket `yscoopy` 経由。リリース手順は yscoopy.md 参照。

## トラブル

| 症状 | 対処 |
|---|---|
| `~/.claude/skills/<name>` が無い | ywatchy 未起動の可能性。`ywatchy` で起動確認 |
| symlink 切れ | source 側削除済か確認。残骸なら手動 rm |
| sync が止まる | プロセス再起動 |
