# タスク: 照合を、表示用の文字列でやめる

`BRIEF.md` と `docs/RULES.md` を読め。

## 直すもの3件

### 1. `src-tauri/src/config_snapshot.rs:197` — 要約で比べている

`state_label` は**画面に出すための要約**。別々の状態が同じラベルを持ちうる。
それで「同じ」と判定すると、**違う設定を同じと言う。**

比較は構造化された値で行え。ラベルは表示にだけ使え。

### 2. `src-tauri/src/config_snapshot.rs:174` — 確認不可の判定が文字列の一部だけ

`unsupported`、`policy_managed`、`error` の `state_kind` を**比較可能として扱っている。**
これらは「確認できた」ではない。`unknown` 側へ入れろ。

### 3. `src-tauri/src/windows/audio.rs:749` — 読み取り失敗を空で返している

エンドポイントの読み取りに失敗したとき空の状態を返すので、
**不完全な控えと 0 件の控えが区別できない。**
失敗は失敗として返せ。`docs/RULES.md` の「0 件が出たら計器を先に疑う」。

## ついでに直せ（同じ監査で出たもの）

- `config_snapshot.rs:157` — `action_id` の重複を検出していない。同じ項目を複数回集計しうる
- `config_snapshot.rs:127` — カードの `version` を対応版と照合していない。未知版を現行形式として解釈する
- `actions/app_volume_reset.rs:211` — 消えたセッション数を捨てている経路がある
- `actions/app_volume_reset.rs:239` — 控えと突き合わせる際、取り違えの余地がないか確かめて必要なら直せ

## 検証

**直したことを、既知の不良を注入して確かめろ。**

- 別状態が同じラベルを持つ場合に「同じ」と言わないこと
- `unsupported` / `policy_managed` / `error` が `matching` に入らないこと
- 読み取り失敗が 0 件と区別できること
- 重複 `action_id` を持つカードが拒否されるか、正しく1回だけ数えられること
- 未知の `version` が拒否されること

各テストについて、**その振る舞いを壊したときに落ちること**を確認して報告しろ。

## 完了条件

- `cargo test --lib` が全部通る（現在376件）
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（CSS構文警告0件）
