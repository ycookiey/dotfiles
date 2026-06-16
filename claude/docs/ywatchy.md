# ywatchy

ファイル監視で symlink/sync を自動管理する Rust 製ツール。scoop shim 経由で常駐。実体は `C:\Main\Project\ywatchy`。

## 役割

- **skill 個別 symlink 自動管理**: `dotfiles/skills/<name>/` を新規作成すると `~/.claude/skills/<name>` への symlink を自動生成 (setup.ps1 への追加不要)
- **project store sync**: 各プロジェクトと `~/.ywatchy/store` を双方向 sync

setup.ps1 管理外。skill ディレクトリだけ作れば残りは ywatchy 担当。

## skill 追加手順

1. `dotfiles/skills/<name>/SKILL.md` 作成 (frontmatter `name`/`description` 必須)
2. 追加リソースを同ディレクトリに配置 (`*.sh`, `*.py` 等)
3. **以上**。setup.ps1 編集不要。ywatchy が `~/.claude/skills/<name>` symlink を張る

確認: `readlink ~/.claude/skills/<name>` で実体パスが出れば成功。

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
