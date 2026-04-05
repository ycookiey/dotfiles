# モバイルアプリ開発

- 個人事業主: ycookiey（Apple Developer / Google Play Developer 加入済）
- デバイス: iPhone X, M1 Mac, Android端末

## Expo Go vs Dev Client（2026-04時点）

### Expo Goでは動かないもの
- **react-native-reanimated v4** / **react-native-gesture-handler v2.28+** / **@gorhom/bottom-sheet v5** — TurboModule互換性エラー (`installTurboModule` argument count mismatch)
- **React 19 + Expo SDK 54** — Expo Go内蔵のreact-native-rendererとバージョン不一致で白画面

### 回避策
- Expo Goで動確したい場合: reanimated/gesture-handler依存を外し、RN標準の `Modal` / `Pressable` / `Animated` で代替（TODOコメント付き）
- 本番想定の動確: `npx expo prebuild` → `npx expo run:android` でdev clientビルド

### pnpm monorepo + Expo の注意点
- **metro.config.js必須**: `watchFolders` にmonorepoルート、`resolver.nodeModulesPaths` でルートnode_modules優先、`disableHierarchicalLookup: true` で重複react-native防止
- **`.npmrc`**: `node-linker=hoisted` + `public-hoist-pattern` にreact/react-native/expo関連を列挙
- ワークスペースパッケージの `react-native` は `peerDependencies` に置く（dependenciesに入れると重複インストール）
- `@types/react` のバージョンも全パッケージで統一（不一致でnode_modules内に重複react-nativeが発生）

### .js拡張子問題
- subagentが生成するTSコードは `from './foo.js'` 形式のインポートを含むことがある
- Expo SDK 54のMetroはこれを解決できず `Unable to resolve` エラー
- 対処: `find packages -name '*.ts' -o -name '*.tsx' | xargs sed -i "s/from '\(.*\)\.js'/from '\1'/g"`

### Android local.properties
- `npx expo prebuild` は `local.properties` を生成しない
- Gradle ビルドに `sdk.dir` が必要。Javaプロパティ形式: `sdk.dir=C\:\\Users\\ycook\\AppData\\Local\\Android\\Sdk`

### dev-android スキル (dev.sh)
- 第1引数: 数字ならPORT、文字列ならAVD名パターン（部分一致検索）
- 非インタラクティブ環境ではExpoの対話プロンプトが失敗するため、最終ステップはコマンド表示のみにした
- `netstat` のgrep失敗を `|| true` でガード（`set -e` 対策）

### React バージョン一致
- react-native 0.81.5 は react-native-renderer 19.1.4 を要求
- package.json の react も 19.1.4 に合わせる必要あり（不一致だと白画面+エラー）

### Windows + エミュレータのMetro chunked encoding問題
- Metro dev serverからエミュレータへのバンドル配信が `ProtocolException: Expected leading [0-9a-fA-F] character but was 0xd` で失敗する
- 原因: Windowsの `\r\n` がHTTP chunked transfer encodingのチャンクサイズ解析で不正文字扱い
- Metroは "Bundled" と報告するがアプリ側は受信失敗 → 白画面（"Bundling 100.0%..."表示のまま）
- `npx expo run:android` でも `npx expo start --dev-client` でも同じ
- 未解決。回避策候補: (1) リリースバンドル埋め込み (2) 物理端末WiFi接続 (3) WSL2からMetro起動

### adb プロセス蓄積
- adbプロセスが大量に蓄積するとadbコマンドがハング
- `taskkill //F //IM adb.exe` → `adb start-server` で解消

### android/ ディレクトリロック（Windows）
- エミュレータ/Gradle使用後、android/ディレクトリがWindowsプロセスにロックされて削除不可になることがある
- `rm -rf`、`cmd /c rmdir`、PowerShell `Remove-Item -Force` いずれも失敗
- PCリブートで解消。prebuildは `--clean` なしでも「malformed」判定で削除を試みるため回避不可

### Expo SDK 54 Metro Web対応
- app.jsonに `"web": { "bundler": "metro" }` が必要（webpack非推奨、`@expo/webpack-config` v19はSDK 54非対応）
- デフォルトHTML は `<script defer>` だがMetroバンドルは `import.meta` を含む → `SyntaxError: Cannot use 'import.meta' outside a module`
- 回避: `public/index.html` に `<script type="module" src="...">` で上書き
- ただしHMR WebSocketが `type="module"` と互換性なくMetroがクラッシュする
- 現時点の動作確認方法: `npx expo export --platform web` → `npx serve dist` で静的配信
