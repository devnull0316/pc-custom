# タスク: Explorer 系の未判定を、確立した型で測る（第2弾）

`BRIEF.md` と `docs/RULES.md` を読め。
`docs/DISPLAY_ONLY_TRIAGE.md` の末尾2節（観測場所の修正、意味のある changed=false）を必ず読め。

## 型は確立している

前回で `nav_show_all` と `drive_letters` が測れた。手順はこう。

1. **その設定が効く場所を先に決める**（左ペイン？ 右一覧？ ツールバー？ 特定のフォルダー？）
2. **その場所を読む観測関数**を用意する。既存の流用が合わなければ新しく足す
3. **校正**: 設定を変える前に、その場所から項目が読めることを確認して数を出す
4. 今と違う値を書く → 観測 → 戻す → 再観測
5. **同じ測定を2回**走らせて同じ結果になることを確認

`EVIDENCE:` に **`calibration_*_count`** を必ず含めろ。**校正の無い `changed=false` は証拠にならない。**

## 対象（3件。自分で選べ）

`docs/DISPLAY_ONLY_TRIAGE.md` の未判定17件から、
**Explorer の窓を開いて観測できるもの**を3件選べ。選んだ理由を書け。

候補になりそうなもの: `explorer.status_bar`、`explorer.preview_handlers`、
`explorer.sharing_wizard`、`explorer.always_show_menus`、`explorer.icons_only`、
`explorer.nav_expand_current`、`explorer.compact_view`（既に測れているなら除く）

**タスクバーに依存するものは選ぶな。** この機は全画面アプリが覆っていて測れない。

## 判定

- 校正が通り、変わった → **昇格候補**として報告
- 校正が通り、変わらなかった → 「実測して効かない」へ格上げ
- **校正が通らない** → 保留。**何が読めなかったかを具体的に書け**
- **測る対象がこの環境に無い** → 保留。何が要るかを書け（例: 空のドライブ）

## 絶対に守ること

- 開いた窓は閉じろ。設定は戻せ。**未設定だったら未設定へ戻せ**
- ウィンドウタイトル・ファイル名を出すな
- **測れなかったものを「効かない」と書くな**

## 完了条件

- `cargo test --lib` が通る
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（テスト23件、CSS警告0件）
- `docs/DISPLAY_ONLY_TRIAGE.md` を更新しろ
