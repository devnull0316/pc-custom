# タスク: 同時に起きたときを監査する

`BRIEF.md` と `docs/RULES.md` を読め。**`docs/STATUS.md` は読まなくてよい。**

## なぜ

これまでの監査は、操作が1つずつ順番に起きる前提だった。
実際には**同時に起きる**。監視スレッドが回っている最中に利用者が別の操作をする。
モードが自動適用している最中にアプリを閉じる。プレビュー中に別のプレビューを作る。

## 見るもの

`src-tauri/src/engine/`、`src-tauri/src/game_profile/watcher.rs`、
`src-tauri/src/taskbar_watcher.rs`、`src-tauri/src/bootstrap.rs`、
`src-tauri/src/journal/`、各 `src-tauri/src/windows/*.rs` の状態を持つもの。

### 探すもの

1. **同じ資源を2つの経路が触る**
   - 監視スレッドと利用者操作が同じ設定を書く経路
   - `mutation_gate` や `acquire_core_mutation_lock` を通っていない書き込み
   - `static` や `OnceLock` や `Mutex` で持っている状態を、ロック外で読んで判断している箇所

2. **判断してから実行するまでに変わりうる**
   - 「現在値を読む → 比べる → 書く」の間に第三者が入れる隙
   - プレビューを作ってから適用するまでに前提が崩れる経路

3. **終了と進行中の処理**
   - 適用の途中でアプリが閉じたら何が残るか
   - `Drop` に頼っている巻き戻しが、別スレッドの進行中処理を待たずに走る箇所

4. **再入**
   - 同じ操作を2回続けて押したときに二重に走る経路
   - ボタンの `disabled` だけで守っていて、バックエンド側に守りが無いもの

## 出し方

FINDING
file: <repo相対パス>:<行番号>
quote: <その行の実際の中身をそのまま1行>
what: <何が壊れるか。1文>
how: <どの2つが同時に起きたときか。具体的に>
severity: high|medium|low
END

**引用する前に、その行が実在することを確かめろ。**
**コードを1行も変更するな。** 推測で埋めるな。無ければ NO FINDINGS。
