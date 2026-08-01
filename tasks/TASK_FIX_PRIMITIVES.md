# タスク: Windows土台とモード実行の10件を直す

`BRIEF.md` と `docs/RULES.md` を読め。

## 最優先: 上限つき一覧で「全部」を探している箇所（high）

**2箇所ある。中核で同じ形を直したばかりで、まだ残っていた。**

- `src-tauri/src/game_profile/manual.rs:96` — `list_timeline(128)` から
  完了済みトランザクションの全項目を探している。**128件を超えると、
  適用したモードが復元記録なしで残る**
- `src-tauri/src/game_profile/engine_sink.rs:58` — `list_timeline(256)` で同じ形

どちらも**件数に依存しない引き方**にしろ。
`transaction_id` で直接引く経路を journal 側に足せ。

**他にも同じ形が無いか、`src-tauri/src/` 全体を自分で探して直せ。**
（テストコードは除く。`commands.rs:137` の画面表示用 250 件は上限として妥当なので触るな）

## その他 high

- `windows/power_mode.rs:216` — 通知登録が失敗したときコールバックが動く可能性があり、
  解放済み `Sender` を参照しうる
- `windows/vpn.rs:511` — 接続直後の検証エラーで、接続を切らずにハンドルを捨てている
- `windows/desktop_icons.rs:204` — PIDL の実割当長を知らずに終端まで走査するので、
  壊れた PIDL で範囲外読み取りになる

## medium / low

- `game_profile/manual.rs:118` — 同一 rollback 内で保存済み項目を再反転し、
  保存失敗時の補償を適用時に失敗させる
- `game_profile/manual.rs:179` — rollback 成功後の保存失敗で、戻した項目が適用中として残る
- `windows/window_placement.rs:1293` — 全ウィンドウ移動後の最終列挙エラーを補償経路に通していない
- `windows/audio.rs:869` — 音量書き込み後のミュート書き込み失敗を補償していない
- `windows/desktop_icons.rs:222` — `S_FALSE` を成功として扱い、自動整列を常に有効と誤観測
- `windows/file_picker.rs:81` — UTF-16 変換失敗時に `CoTaskMemFree` を呼ばず return

## 検証

**各件について、直したことを示すテストを足せ。**
そして**そのテストが、直す前の実装で落ちること**を確かめて報告しろ。

特に上限の2件は、**上限を超える件数を実際に作って**確かめろ。

## 完了条件

- `cargo test --lib` が全部通る（現在394件）
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（CSS構文警告0件）
