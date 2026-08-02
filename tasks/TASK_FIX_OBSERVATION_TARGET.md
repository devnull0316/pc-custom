# タスク: 観測の場所を、設定が効く場所に合わせる

`BRIEF.md` と `docs/RULES.md` を読め。
`docs/DISPLAY_ONLY_TRIAGE.md` の末尾「Explorer 系3件の測定」を必ず読め。

## 分かっていること

2件とも `changed=false` だったが、**観測が設定の効く場所を見ていなかった。**

| 項目 | 設定が効く場所 | いま観測している場所 |
|---|---|---|
| `explorer.nav_show_all` | 左のナビゲーションペイン | 右のファイル一覧 |
| `explorer.drive_letters` | 「PC」のドライブ名 | 一時フォルダーの中身（ドライブが無い） |

**測り方の誤りであって、設定が効かない証拠ではない。**

## やること

### 1. `explorer.nav_show_all`

`NavPaneShowAllFolders` を有効にすると、左ペインに
「ごみ箱」「コントロール パネル」などが**追加で現れる**。

- 観測を**左ペイン**に変えろ。UIA でツリー（`Tree` / `TreeItem`）を探せ
- `explorer_window_item_names` は右の一覧用。**流用するな。** 左ペイン用の関数を足せ
- **校正しろ**: 設定を変える前後で左ペインの項目数が取れることを先に確かめろ。
  取れないなら `measured=false reason=nav_pane_not_readable` を出して止まれ

### 2. `explorer.drive_letters`

`ShowDriveLettersFirst` はドライブの表示名の順序を変える
（「Windows (C:)」と「(C:) Windows」）。

- 観測を「**PC**」を開いた状態に変えろ。一時フォルダーではドライブが出ない
- `shell:MyComputerFolder` で開けば確実
- 項目名に `(C:)` が**先頭にあるか末尾にあるか**を見ろ
- **校正しろ**: ドライブ項目が1つ以上見つかることを先に確かめろ。
  見つからないなら `measured=false reason=no_drive_items` を出して止まれ

## 判定

- 変わった → **昇格候補**として報告
- 変わらなかった（校正は通った）→ 「実測して効かない」へ格上げ
- 校正が通らない → 保留のまま。試した観測を記録しろ

**同じ測定を2回走らせて、同じ結果になることを確かめてから報告しろ。**

## 絶対に守ること

- 開いた窓は閉じろ。設定は戻せ。**未設定だったら未設定へ戻せ**
- `EVIDENCE:` に**校正の結果**（左ペインの項目数、ドライブ項目の数）を必ず含めろ
- ウィンドウタイトル・ファイル名を出すな

## 完了条件

- `cargo test --lib` が通る
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（テスト23件、CSS警告0件）
- `docs/DISPLAY_ONLY_TRIAGE.md` を更新しろ
