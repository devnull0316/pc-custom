# タスク: 12時間 / 24時間表示を一時切替

`BRIEF.md` が契約。`docs/STATUS.md` を読むこと。
`docs/RESEARCH_FEATURES_ROUND3.md` の「5. 12時間 / 24時間表示を一時切替」を実装する。

## 作るもの

Action を1つ。現在のユーザーの**短い時刻表示**を 12 時間または 24 時間へ切り替える。

## 約束しないこと

日付、地域、タイムゾーン、システムアカウント、ロック画面は**変わらない**。
そう変わるとは書かない。作用範囲どおりに書く。

画面には**プレビューを出す**。「13:05 で表示」「1:05 PM で表示」。
内部の書式文字列（`HH:mm` など）を画面に出さない。

## 必ず守ること

- 適用前の書式文字列をそのまま保存し、**その文字列へ戻す**。既定値を書かない
- 戻すとき現在値が自分の適用値と違えば `ExternalConflict` で止める
- ロケールによって AM/PM の文字は違う。**固定の英字を期待しない**

## 検証（自分で書いた「8) 外から効いたとどう測るか」をそのまま実行する）

1. **別の新規プロセス**で `GetTimeFormatEx` に固定時刻 13:05 を渡し、期待の表記になることを確認
2. タスクバーの時計を UI Automation で読み、24時間なら 13、12時間なら 1 と marker 相当が見えることを確認
3. ロールバック後、**また新しいプロセス**で同じ固定時刻を整形し、元の表記と一致することを確認
4. **タスクバーが更新しない環境では setter の readback だけで成功にしない。**
   「設定値は変更済みだがシェル表示を確認できない」と分けて出す

`EVIDENCE:` 行に各時点の整形結果を出すこと。
測れないときは `measured=false reason=...` と出すこと。**黙って return しない。**

## 登録し忘れが致命的になる箇所

- `ActionParameters` の変種に `#[serde(rename = "...")]` を付ける（付け忘れると画面から呼べない）
- `presentation.rs` の `category_for` が画面の知るカテゴリを返す
- `action/id.rs` の `ActionId` にも `#[serde(rename)]` を付ける（耐久バックアップの表記）
- ACL toml、`invoke_handler`、`parametersForAction`、`catalog.ts`
- 件数テスト 72→73、README の内訳表、CHANGELOG

`cargo test --lib request_round_trip` `category_contract` `count_report` が見張っている。

## 完了条件

- `cargo test --lib` 全通過（現在340件）
- `cargo clippy --all-targets -- -D warnings` 通過
- `cargo fmt --check` 通過
- `npm run build` 通過（CSS構文警告0件のまま）
- 実機テストを実際に走らせて `EVIDENCE:` 行を貼ること

ビルドが通らないコードを返さないこと。
