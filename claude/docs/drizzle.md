# drizzle-kit (SQLite / D1)

## テーブル作り直し migration の罠

- 発生: 列の NOT NULL 変更等 → drizzle-kit generate が `__new_<table>` 作成 → INSERT…SELECT → DROP → RENAME の SQL を生成
- 罠1: INSERT…SELECT が新規列まで旧テーブルから SELECT → データ入り DB で `no such column`。空 DB のテストでは気づけない
- 罠2: 複数テーブルを作り直すとき、1 つ目の後で `PRAGMA foreign_keys=ON` に戻す → 親の `DROP TABLE` の暗黙 DELETE が ON DELETE CASCADE を発火 → 子の行が全消失
- 対策: 生成 SQL を必ず読む。新規列は SELECT 側で値を与える、FK は全テーブル作り直し後まで OFF、手修正は先頭コメントに明記
- 検証: 旧 migration まで適用 → 行を入れる → 新 migration 適用 → 行が残ることをテスト
- D1: FK を OFF にできない → データ入り D1 で作り直し migration を流す前に local D1 で行が残るか確認(回避手順は未確立)
