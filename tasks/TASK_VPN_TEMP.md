# タスク: 仕事用VPNを一時接続

`BRIEF.md` と `docs/RULES.md` を読め。**`docs/STATUS.md` は読まなくてよい。**
`docs/RESEARCH_FEATURES_ROUND3.md` の「6. 仕事用 VPN を一時接続」を実装する。

## 作るもの

**既に Windows に登録されている** VPN 接続を1つ選び、モードの間だけ繋ぐ。終わったら元の状態へ戻す。

## やらないこと

- VPN 接続を**新規作成しない**。資格情報を一切扱わない
- 認証情報を保存も入力もしない
- 接続先アドレスや利用者名を**ログにも EVIDENCE にも出さない**（社内情報）
- 既に繋がっている接続を勝手に切らない

## 必ず守ること

- 適用前に「その接続が繋がっているか」を読み、**元の状態を保存**する
- 自分が繋いだ場合だけ切る。**元から繋がっていたなら何もしない**（出番なしと表示）
- 戻すとき、現在の状態が自分の適用値と違っていたら `ExternalConflict` で止める
- 資格情報が必要で接続できない場合、**利用者に Windows の画面を案内する**（guided）
- 名前は利用者が付けたものなので画面には出してよい。**ログには出さない**

## 使えるもの

`src-tauri/src/windows/` の既存の COM / Win32 の作法に従うこと。
RAS API（`RasEnumConnectionsW` / `RasEnumEntriesW` / `RasDialW` / `RasHangUpW`）は文書化されている。
**`RasDialW` に資格情報を渡す実装をするな。** 保存済み資格情報でのみ接続を試み、
足りなければ Windows の設定画面へ案内する。

## 検証

`#[ignore]` の実機テストで、次を `EVIDENCE:` 行に出す。**接続名は出すな。ハッシュか件数だけ。**

1. 登録済み接続の件数を読む
2. **0件なら「測れない」と出して終わる**（成功と区別できるように）
3. 元から繋がっている接続があれば「出番なし」と出して終わる
4. 繋げられる場合のみ、接続 → 読み直して繋がったことを確認 → 切断 → 読み直して元に戻ったことを確認

**同じ状態を書いて同じ状態が返っても何も証明していない。**

## 完了条件

- `cargo test --lib` が全部通る（現在344件）
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（CSS構文警告0件のまま）
- `request_round_trip` `category_contract` `count_report` が通る
- Action を増やすなら件数テスト・README・CHANGELOG も直す
- 実機テストを**実際に走らせて** `EVIDENCE:` 行を貼る
