# React

## React Compiler lint: ref を含むオブジェクトを props に渡すとエラー

- 症状: eslint-plugin-react-hooks の `react-hooks/refs` が `Cannot access refs during render` を JSX ごとに出す
- 原因: custom hook の戻り値など、`useRef` の ref を含むオブジェクトを丸ごと子の props に渡した(`<Child editor={editor} />`)。子が `.current` を読むかを lint は区別できない
- 対策: 状態・操作・ref を別々の props に分け、ref は ref 用の props(`ref={ref}` / `headingRef={ref}`)で渡す。ref を含まないオブジェクト(状態・操作)は丸ごと渡しても通る
- 実測: 管理画面の詳細で 12 件(2026-09-30)
