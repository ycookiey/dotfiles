# drizzle-kit (SQLite / D1)

## テーブル作り直し migration の罠

- 発生: 列の NOT NULL 変更等 → drizzle-kit generate が `__new_<table>` 作成 → INSERT…SELECT → DROP → RENAME の SQL を生成
- 罠1: INSERT…SELECT が新規列まで旧テーブルから SELECT → データ入り DB で `no such column`。空 DB のテストでは気づけない
- 罠2: 複数テーブルを作り直すとき、1 つ目の後で `PRAGMA foreign_keys=ON` に戻す → 親の `DROP TABLE` の暗黙 DELETE が ON DELETE CASCADE を発火 → 子の行が全消失
- 対策: 生成 SQL を必ず読む。新規列は SELECT 側で値を与える、FK は全テーブル作り直し後まで OFF、手修正は先頭コメントに明記
- 検証: 旧 migration まで適用 → 行を入れる → 新 migration 適用 → 行が残ることをテスト
- D1: FK を OFF にできない → データ入り D1 で作り直し migration を流す前に local D1 で行が残るか確認(回避手順は未確立)

## D1 の batch に `db.run(sql)` を入れられない

- 症状: ``db.batch([db.run(sql`…`), …])`` → `TypeError: Cannot read properties of undefined (reading 'bind')`(drizzle-orm 0.45 の d1 session)
- 対処: 条件付きの挿入は ``db.insert(table).select(sql`SELECT … WHERE EXISTS (…)`)`` で書く。挿入の列はテーブル定義順(列の並びに SELECT をそろえる)
- 応用: 版の照合を batch 1 回で行うには、一意 ID の行を「条件が合うときだけ」先に挿入し、後続の update を `exists(その行)` で絞る。あとで行の有無を読めば、適用されたか判定できる

## generate の meta JSON が Prettier 未整形

- drizzle-kit generate の `migrations/meta/_journal.json`・`NNNN_snapshot.json` は prettier の書式でない → CI の format:check で落ちる
- 対処: 生成物は整形しない。`.prettierignore` に meta の dir(`migrations/meta`・`packages/*/drizzle/meta` 等)を入れ、generate の出力のまま commit
  - generate の script に `&& prettier --write` を足す形は不可: CI の差の確認(`db:generate --name <x>`)の引数が末尾の prettier に渡って壊れる
