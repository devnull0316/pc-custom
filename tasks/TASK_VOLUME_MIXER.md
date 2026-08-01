# タスク: アプリごとの音量を、いじる前へ戻す

`BRIEF.md` と `docs/RULES.md` を読め。
`docs/RESEARCH_FEATURES_ROUND4.md` の2位「アプリ別音量ミキサーの調整前一括リセット」を実装する。

## 作るもの

配信・会議・ゲームの前に、いまのアプリ別音量を**控える**。終わったら控えた値へ**戻す**。

## なぜ作れるか（ここが要）

`ISimpleAudioVolume` は `GetMasterVolume` / `SetMasterVolume` と
`GetMute` / `SetMute` が**対称に公開されている**。
だから正確に戻せる。RGB 照明を却下したのは getter が無かったからで、ここは逆。

## 必ず守ること

- **控えていない状態から「戻す」を出すな。** 控えが無ければ「控えがありません」と言え
- 戻す先は**控えた値**。既定値や 100% を書くな
- 控えた後に**新しく現れたアプリは触るな**。控えに無いものは範囲外
- 控えた後に**消えたアプリ**は、戻せなかったものとして数に出せ。黙って0件に混ぜるな
- 現在値が自分の適用値と違うなら `ExternalConflict` で止めろ。取り返すな
- **アプリ名・実行ファイルパスをログにも EVIDENCE にも出すな。** 既存の `application_label` の作法に従え
- システム音量（マスター）は触るな。アプリ別だけ

## 使えるもの

`src-tauri/src/windows/audio.rs` に Core Audio の作法が既にある。**まず読め。**
COM は専用スレッドで初期化する既存の形に従え。

`IAudioSessionManager2::GetSessionEnumerator` → `IAudioSessionControl2` →
`ISimpleAudioVolume` が文書化された経路。

## 検証

`#[ignore]` の実機テストで:

1. 現在のセッション数と、各セッションの音量を読む
2. **セッションが0件なら「測れない」と出して終われ**（成功と区別できるように）
3. 自分で音を鳴らすプロセスを用意できないなら、**既存セッションのうち1つだけ**を対象に、
   今と違う値へ変える → 読み直す → 戻す → 読み直す
4. 元と同じ値を書いて同じ値が返っても**何も証明していない**。必ず違う値にしろ

`EVIDENCE:` 行に、セッション数と、前・変更後・戻し後の**音量値だけ**を出せ。**名前は出すな。**
panic しても戻るよう `Drop` を使え。**利用者本人の音量設定だ。**

## 完了条件

- `cargo test --lib` が全部通る（現在365件）
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（CSS構文警告0件）
- `request_round_trip` `category_contract` `count_report` が通る
- Action を増やすなら件数テスト・README・CHANGELOG も直せ
- 実機テストを**実際に走らせて** `EVIDENCE:` 行を貼れ
