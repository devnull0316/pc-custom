# タスク: 前の版から上げたときを監査する

`BRIEF.md` と `docs/RULES.md` を読め。**`docs/STATUS.md` は読まなくてよい。**

## なぜ

このアプリは**利用者の変更を元へ戻すための記録**を持っている。
版が上がったとき、その記録が読めなくなると、戻せなくなる。
新規インストールでしか試していない。

## 見るもの

`src-tauri/migrations/`、`src-tauri/src/journal/`、`src-tauri/src/backup/`、
`src-tauri/src/action/id.rs`、`src-tauri/src/action/parameters.rs`、
`src-tauri/src/game_profile/store.rs`、`src-tauri/src/taskbar_watcher.rs`、
その他 JSON をディスクへ書いている箇所。

### 探すもの

1. **古い記録が読めなくなる形**
   - `#[serde(deny_unknown_fields)]` が付いた型に、新しい欄を足していないか
   - `ActionId` や `ActionParameters` の `rename` を変えた形跡はないか
     （耐久記録の中の文字列が変わると、そのバックアップは復号できない）
   - `enum` に新しい値を足したとき、古い版が書いた値を読めるか / 逆はどうか
   - バックアップの `codec_version` と `rollback_decoder_versions` の対応が抜けている Action はないか

2. **設定ファイルの版**
   - JSON を書いている各所に版番号があるか。無いものはどれか
   - 版が上がったときの読み方が書いてあるか。無ければ、次に足したとき何が起きるか

3. **マイグレーション**
   - `migrations/` の SQL が、既存 DB に対して安全か（`IF NOT EXISTS` 等）
   - 新しい列を足したとき、古い行に何が入るか
   - **失敗したときに記録が壊れないか**

4. **ダウングレード**
   - 新しい版が書いた記録を、古い版が読んだらどうなるか
   - 壊れるなら、壊れると分かる形で止まるか（黙って誤読しないか）

## 出し方

FINDING
file: <repo相対パス>:<行番号>
quote: <その行の実際の中身をそのまま1行>
what: <何が読めなくなるか。1文>
how: <どの版からどの版へ上げたときか。具体的に>
severity: high|medium|low
END

**引用する前に、その行が実在することを確かめろ。**
**コードを1行も変更するな。** 無ければ NO FINDINGS。
