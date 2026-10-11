# League of Legends のゲームデータ取得 (地形・配置)

位置や地形を画像 (minimap の screenshot、wiki の地図) から推定しない。ゲームファイルに正解がある。
実例: minimap の色から作った壁は多くの場所で約 1 cell (50 units) ずれていた。wiki 由来の camp 中心は最大 235 units ずれ、赤バフが壁の中に入っていた。

## 地形 (通行判定) = navgrid

- ファイル: `Map11.wad.client` (例 `C:/Riot Games/League of Legends/Game/DATA/FINAL/Maps/Shipping/`) 内の `assets/maps/navgrid/map11/aipath_srx_2.aimesh_ngrid`
  - どれを使うかは `data/maps/shipping/map11/map11.bin` の `MapSkins/Default > mNavigationMesh` で確かめる (候補は複数ある: aipath / _srx / _srx_2 / _srx_3)
  - `NavGrid_*.ngrid_overlay` はドラゴンソウル地形と Atakhan 用。序盤は無関係
  - 第三者の変換済み画像 (LoL-NGRID-converter の `SR_2024`) は古い。今の client から取る
- 形式: v7、50 units 四方の cell が 295×296。cell 48 byte の後に VisionPathingFlags (u16) 列、その後に region (4 byte/cell)。仕様は FrankTheBoxMonster/LoL-NGRID-converter `NGridFileReader.cs`
  - 通行不可 = flag `2` (wall) または `4` (structure)。`64` (透明壁) は wall bit と併記される。単独の `64` は team 専用の base gate
  - flag `128` は広く散在する (SR で約 1.8 万 cell)。変換ツールの作者は「意味不明・無視」としている。通行不可には含めず、その結果の地形がユーザー提供の minimap と一致することは目視で確認済み (128 単独の描画による検証はしていない)
- ゲームの通行判定はこの cell 単位。見た目の滑らかさは描画側で付ける (輪郭を Chaikin で丸める等)。判定を丸めない

## jungle camp / 個体の spawn 位置

- `data/maps/mapgeometry/map11/base_srx.materials.bin` の `MapPlaceableContainer` `Maps/MapGeometry/Map11/Chunks/SRS_Jungle`
  - `Character` を持つ placeable = 個体の spawn 位置 (`transform[3]` の x, z。y は高さ)。`NeutralCamp` を持つもの = camp 位置
  - 実在する個体の向きもある (`transform` の回転行列)
- 解析済み JSON が CommunityDragon に version 固定 URL で置かれている: `https://raw.communitydragon.org/<major.minor>/game/data/maps/mapgeometry/map11/base_srx.materials.bin.json`
  - 手元 client の version は `Game/content-metadata.json`。`raw.communitydragon.org/<ver>/content-metadata.json` と照合する
- 壁の縁に置かれた個体もある (例: blue side Crimson Raptor は壁 cell の 14 units 内側)。データ通りに使う

## WAD (`.wad.client`) の読み方

- v3.x: header 272 byte (magic `RW`, ver, 256 byte 署名, checksum, entry 数 u32 @268)、entry 32 byte (path hash u64, offset u32, 圧縮長 u32, 長さ u32, type 下位 4 bit …)
- path hash = xxHash64(seed 0) of 小文字 path。path と hash の対応表: `https://raw.communitydragon.org/data/hashes/lol/hashes.game.txt` (230 MB、grep で引く)
- type 3/4 は zstd。Node 22.15+ / 23.8+ の `zlib.zstdDecompressSync` で展開できる (古い `@types/node` には型が無いのでキャストする)
- 実装例: AnalyzerLoL `scripts/lib/wad.ts`、`scripts/lib/ngrid.ts`、`scripts/build-sr-map.ts`、`scripts/build-jungle-spawns.ts`
- CommunityDragon の `files.exported.txt` に navgrid は無い (展開対象外)。navgrid は手元の client から取る

## 建物・ミニオン・視界・出現時刻

- 建物・ミニオンの通り道・出撃地点: `base_srx.materials.bin` の `Maps/MapGeometry/Map11/Chunks/SRS_Structures`
  - 建物は `Character.CharacterRecord` で種類が分かる (`Turret/CharacterRecords/SR_Outer|SR_Inner|SR_Inhibitor|SR_Nexus`、`Inhibitor/`、`Nexus/`、`SRUAP_Turret_Order5` = 泉)
  - `MinionPath_Top|Mid|Bot`: `Segments` は transform の原点からの相対 (x, z)。出撃地点 (`Barracks`) の `LinkedPath` がこの placeable の key
  - `Barracks.BarracksConfig` は `map11.bin` の entry の hash。`InitialSpawnTimeSecs` 30 / `WaveSpawnIntervalSecs` 30 / `MinionSpawnIntervalSecs` 0.8、`units` の並び = 出撃順 (MinionType 4 近接・6 大砲・5 遠距離・7 スーパー)
- character bin (`raw.communitydragon.org/<ver>/game/data/characters/<name>/<name>.bin.json`) は CharacterRecord を複数持ち、record ごとに値が違う。SR の値は key `Characters/<Name>/CharacterRecords/Root` の record。`grep -o … | head -1` で先頭を拾わない
  - 実例 (16.20 HP): sru_red Root 2300 / 別 record 5000、sru_blue Root 2300 / 3750、sru_razorbeak Root 1200 / 1600、sru_krug Root 1400 / 1650。hook `cdragon-root-record-reminder.sh` が該当 URL の直前に差し込む
- 視界半径: character bin の Root record の `perceptionBubbleRadius`。既定値のときは項目が無い (Neeko・タワー) → wiki の Sight ページ (チャンピオン・タワー 1350、ミニオン 1200) で補う
- 移動速度: `baseMoveSpeedModifiable.baseValue` (ミニオン 350)
- camp の初回出現時刻はゲームデータ (bin) からは取れない (`NeutralCamp` にあるのはミニマップのタイマー表示設定だけ)。wiki の各 monster ページの patch history で確認する (V26.01: 0:55、Gromp・Krugs 1:07)
