# ESLint

## eslintrc(ESLint 8)の parser・plugin は、それを書いた設定の場所から読まれる

- 現象: 共有の設定(`extends: ["@scope/eslint-config"]`)が `parser: "@typescript-eslint/parser"` を書くと、使われるのは共有の設定の依存の版。root の devDependencies に別の版があっても使われない
- 実測: root は 8.70、lint が使っていたのは共有の設定の依存の 7.18。7.18 にない `parserOptions.projectService` を書くと "You have used a rule which requires parserServices to be generated" で止まった
- 確かめ方: root から次を流し、共有の設定の場所から見た版を読む

  ```sh
  node -e 'const r=require("module").createRequire(require.resolve("@scope/eslint-config")); for (const p of ["@typescript-eslint/parser","@typescript-eslint/eslint-plugin"]) console.log(p, r(p + "/package.json").version)'
  ```

- 版を上げるのは共有の設定の側(依存の範囲を上げて公開)。root に足しても効かない

## typescript-eslint の型情報: `project` の列挙と `projectService`

- `parserOptions.project` に tsconfig を並べると、tsconfig ごとに TypeScript の program を作る。package が増えると Node の heap(CI で約 2GB)を超えて `JavaScript heap out of memory` で落ちる
- `projectService: true` は各 file の型を、いちばん近い `tsconfig.json` から 1 つの TypeScript の service で読む。実測: heap の下限が 1792〜2048MB → 1280〜1536MB、CI の lint の段が 23 秒(切り替え前は、package が 1 つ少ない成功した run で 51 秒。10 package と example の monorepo)
- 正式な名前は 8 系から。7 系は `EXPERIMENTAL_useProjectService`(8 系ではこの名前は効かない)
- service が探すのは `tsconfig.json` という名前の file だけ。`tsconfig.worker.json` などにしか入らない file は、8 系では lint のエラー "was not found by the project service" になる。7 系の試験版は、strict でない既定の設定で黙って読む(debug の `configFileName: undefined`)。対処: その場所に `tsconfig.json` を置き、中身は該当の設定を `extends` するだけにする(`include` は extends 元の場所から解決されるので範囲は変わらない)
- 切り替えの確かめ: 指摘 0 件どうしの比較では、型を読めているかは分からない。型の情報を使う rule にわざと引っかかる行を、tsconfig の区分ごとに入れ、前後とも指摘されるかを見る。割り当ては `DEBUG='typescript-eslint:typescript-estree:useProgramFromProjectService'` の `configFileName` で数える
