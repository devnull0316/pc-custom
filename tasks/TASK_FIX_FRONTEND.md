# タスク: 画面側の論理の11件を直す

`BRIEF.md` と `docs/RULES.md` を読め。**Rust には触るな。`src/` だけ。**

## high 7件

1. `src/components/ActionBrowser.tsx:127` — 絞り込みから外れた選択済み Action が詳細に残り、
   **表示中の結果とは別の Action をプレビューできる。** 押した相手と適用される相手が違う

2. `src/App.tsx:375` — プレビュー要求に世代を通していない。
   **遅れて完了した古い要求が、新しいプレビューを上書きする**

3. `src/App.tsx:503` — `confirmTrial` の真偽値を確認せず、
   **保存できなかったのに保存したことにしている**

4. `src/App.tsx:481` — 複数項目の復元が途中で失敗しても、
   既に復元した項目を `justApplied` から除かないので再実行できてしまう

5. `src/components/ProfilesView.tsx:124` — 確認済みプレビューと現在の入力が結び付いていない。
   **未確認の JSON を取り込める**

6. `src/components/ThemeSchedulePanel.tsx:49` — 取得失敗時に編集可能な既定値へ置き換えるので、
   **存在する設定を利用者が上書きできる**（`docs/RULES.md` の「読めなかったを既定値にしない」）

7. `src/components/TempCleanupPanel.tsx:80` — 削除確定ボタンが `dataMode` を見ていない。
   **catalog へ移行した後でも実行できる**

## medium 4件

8. `src/App.tsx:227` — モード一覧の取得失敗を空配列にしている。
   **登録済みモードが存在しないように見える**（同じ形を昨日直したばかり）

9. `src/components/ProfilesView.tsx:191` — 作成結果を待たずフォームを消すので、失敗時に入力が戻らない
10. `src/components/StorageHistoryPanel.tsx:81` — 2つの取得に世代管理がなく、古い結果が新しいものを上書きする
11. `src/components/StorageHistoryPanel.tsx:209` — 検証なしの `toISOString` で、範囲外の値で描画が例外終了する

## 直し方の指針

- **世代管理は1つの形に統一しろ。** 3箇所で別々の書き方をするな
- 失敗を空配列・既定値へ変換するな。**失敗は失敗として保つ**
- `disabled` だけで守るな。実行側でも `dataMode` を確かめろ

## 検証

**各件について、直したことを示すテストを足せ。**
テストの土台が `src/` に無いなら、**まず最小限の仕組みを入れてよい**
（`vitest` を追加してよい。ただし `npm run build` と `npm run typecheck` を壊すな）。

テストが書けない項目は、**書けない理由を報告に書け。** 黙って飛ばすな。

## 完了条件

- `npm run build` が通る（CSS構文警告0件）
- `npm run typecheck` が通る
- `cargo test --lib` が通る（Rust を触らないので405件のまま）
