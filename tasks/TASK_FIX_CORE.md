# タスク: 中核で見つかった5件を直す

`BRIEF.md` と `docs/RULES.md` を読め。
**engine / journal / backup は全 Action の土台。ここが壊れると全部壊れる。**

## 1. `engine/transaction.rs:81` — 確認後に状態が変わる（high）

`preview.before` と現在を突き合わせた**後**に backup を取っている。
その間に第三者が変えると、**変わった後の状態を新しい前提として取り込む。**
利用者が確認した状態とは別のものを上書きすることになる。

backup を取った後にもう一度確かめるか、確認と取得を1つの守りの中へ入れろ。
**どちらにせよ、利用者が見た状態と違うものを適用するな。**

## 2. `engine/transaction.rs:22` — 登録が非原子的（high）

Action の実行と使用登録が別々なので、**登録に失敗すると、記録に無い適用済み状態が残る。**
戻す手段が無い変更ができてしまう。

登録できないなら適用するな。適用したなら必ず記録しろ。順序を見直せ。

## 3. `journal/repository.rs:253` — ロールバックの巻き込み（high）

1件の item を戻すと transaction が `ROLLING_BACK` のまま残る。
その後の reconcile が、**同じ transaction の他の item まで戻す。**
利用者は1件だけ戻したのに、まとめて戻る。

item 単位の戻しと transaction 単位の戻しを混ぜるな。

## 4. `engine/recovery.rs:124` — 200件の壁（high）

期限切れの試用を探すのに `list_timeline(200)` を使っている。
**200件を超えると、古い試用が見つからず自動で戻らない。**
「試して、決めなければ戻る」という約束が静かに破れる。

件数に依存しない探し方にしろ。試用の記録から直接引け。

## 5. `journal/repository.rs:629` — 整合性検証を通していない（medium）

`applied_backups` が `integrity_sha256` を検証していないので、
**壊れた backup でも parse さえ通れば照合の基準になる。**

検証しろ。通らないものは基準にせず、`unreadable` 側へ数えろ。

## 検証

**各件について、直したことを示すテストを足せ。**
そして**そのテストが、直す前の実装で落ちること**を確かめて報告しろ。
落ちないなら、そのテストは何も見ていない。

特に 4 は、**200件を超える記録を作って**、古い試用が見つかることを確かめろ。

## 完了条件

- `cargo test --lib` が全部通る（現在389件）
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（CSS構文警告0件）
