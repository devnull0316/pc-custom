# タスク: コントラストテーマの試用を Action として出す

`BRIEF.md` が契約。`docs/STATUS.md` の末尾「高コントラストは『比が上がる』わけではない」を読め。

## 前提

測定は**もう成立している**。`high_contrast_changes_separate_process_pixels_and_restores` が
`measured=true` で通る。別プロセスのピクセルが動き、正確に戻ることを実測済み。

残っているのは**Action として出すこと**だけ。測定を作り直すな。

## 言い方（ここが本題）

**「読みやすくなる」と書くな。** 測っていない。比はむしろ下がる（18.4→16.3）。

書いてよいのは次まで。
- 見え方が変わる
- 15〜30秒で自動的に元へ戻る
- いつでもすぐ戻せる
- 元の値へ正確に戻る

書いてはいけない言葉: 読みやすく／見やすく／改善／最適／おすすめ／目に優しい／アクセシビリティ診断。
**テストで固定しろ。** 既存の `share_session.rs` の禁止語テストと同じ形にする。

## 必ず守ること

- **開始時点で既に高コントラストが有効なら、何もしない。**「出番なし」と出す。
  常用している人の設定を勝手に触らない
- 適用前の `HIGHCONTRASTW` 全フィールドと scheme 名を保存し、**その値へ戻す**
- 時間切れで自動的に戻る。戻らないまま残さない
- 終了時、現在値が自分の適用値と違えば上書きしない（`ExternalConflict`）
- 既存の preview → commit → journal を通す。**自前の適用ループを書くな**

## 登録

`ActionId` と `ActionParameters` の**両方**に `#[serde(rename)]` を付ける。
ACL toml、`invoke_handler`、`parametersForAction`、`catalog.ts`、
件数テスト 72→73、README の内訳表、CHANGELOG。

`request_round_trip` `category_contract` `count_report` が見張っている。

## 完了条件

- `cargo test --lib` 全通過（現在340件）
- `cargo clippy --all-targets -- -D warnings` 通過
- `cargo fmt --check` 通過
- `npm run build` 通過（CSS構文警告0件のまま）
- 禁止語テストがあること
- 既存の `high_contrast_changes_separate_process_pixels_and_restores` が通ったままであること

ビルドが通らないコードを返さないこと。
