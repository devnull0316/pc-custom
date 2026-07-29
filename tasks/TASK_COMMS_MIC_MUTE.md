# タスク: 既定の通話マイクをミュートする

`BRIEF.md` が契約。`docs/STATUS.md` の失敗記録を先に読むこと。
自分が書いた `docs/RESEARCH_FEATURES_ROUND2.md` の「候補1」を実装する。

## 作るもの

Action を1つ。`audio.comms_mic_mute`。
`eCommunications` の既定の**入力**エンドポイント1台だけをミュートする。

## 名前と言い方

**「すべてのマイクをミュート」とは書かない。** 実際の作用範囲どおりに書く。
別の入力端末を明示指定しているアプリには効かない。排他モードの端末にも効かない可能性がある。
自分の調査文書にそう書いたのだから、画面にもそう書くこと。

## 必ず守ること

### 対象は1台。同じ1台にだけ戻す

適用直前に **device ID と `GetMute` の値**を保存する。
戻すときは**その device ID のエンドポイントにだけ**書く。

**既定端末が入れ替わっていたら、新しいほうを触らない。** 触ったら別の機器を勝手に変えることになる。
対象が抜かれている場合は、上書きせず「戻せていない」として残すこと。

### 第三者の変更を上書きしない

戻すとき、現在の mute 値が自分の適用値と違っていたら `ExternalConflict` で止める。
既存の Action がどうしているか読んで、同じ形にする。
`src-tauri/src/actions/shift_interruption_guard.rs` が近い。

### 効果を保証しない

`EndpointVolume` の公式文書は「不適切な利用は利用者の音量設定を乱し得る」と警告している。
排他モードのストリームには software mute が効かないことがある。
**「これで確実に無音」と書かない。** 設定値としてミュートにしたこと以上を言わない。

### 既存のもの

`src-tauri/src/windows/audio.rs` に既に音声関係の読み取りがある。**まず読むこと。**
COM の初期化の作法は `src-tauri/src/windows/update_status.rs`（専用スレッド）と
`src-tauri/src/windows/desktop_icons.rs`（STA）にある。同じ形にする。

Action の骨格は `src-tauri/src/actions/pointer_feel.rs` が最も近い。
`BackupPayload` に新しい種別を足す形も同じ。

### 登録し忘れが致命的になる箇所

このセッションで2回踏んだ。**両方やること。**

1. `ActionParameters` の新しい変種に `#[serde(rename = "audio.comms_mic_mute")]` を付ける。
   付け忘れると画面から一切呼べない。コンパイルは通る。
2. `presentation.rs` の `category_for` が返す値が、画面の知っているカテゴリであること。

`cargo test --lib request_round_trip` と `cargo test --lib category_contract` が
その2つを見張っている。**必ず通すこと。**

登録先: `action/id.rs`、`action/parameters.rs`、`action/registry.rs`、`presentation.rs` の各 match、
`src/App.tsx` の `parametersForAction`、`src/catalog.ts`。
`presentation.rs` の件数テストは 70 → 71 に直す。README と CHANGELOG の数字も直す。

## 検証（ここが本題）

**書けたことを効果の証明にしない。** このプロジェクトはそれで何度も失敗している。

`#[ignore]` の実機テストを書き、次を確認して `EVIDENCE:` 行に出すこと。

1. 適用前の device ID と mute 値を読む
2. 適用する
3. **同じ device ID を読み直して** mute が true になったことを確認する
4. ロールバックする
5. **もう一度読み直して** 元の値に戻ったことを確認する

**元の値がすでに mute だった場合、同じ値を書いて同じ値が返っても何も証明していない。**
そのときは「測れなかった」と出力して、成功と区別できるようにすること。
このセッションで電源モードの往復テストが同じ間違いをして、緑のまま何も証明していなかった。

テストは panic しても元へ戻すこと（`Drop` を使う）。利用者本人のマイク設定であることを忘れない。

## 完了条件

- `cargo test --lib` が全部通る（現在301件）
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る
- 実機テストを**実際に走らせて** `EVIDENCE:` 行を報告に貼ること

ビルドが通らないコードを返さないこと。
