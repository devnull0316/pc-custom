# タスク: 効くと実測できた項目を、実際に使えるようにする

`BRIEF.md` と `docs/RULES.md` を読め。

## 何が分かったか

`explorer.launch_target` は**実測で効くと確認できた**。

```
measured=true before=2 written=1 restored=2 changed=true restored_ok=true
```

引数なしで開いた新しいエクスプローラーが「ホーム」から「PC」へ変わり、戻した。
**それでもまだ表示専用のまま。** 効くと分かった項目が使えないのは、この製品の趣旨に反する。

## やること

`explorer.launch_target` を**変更できる項目へ昇格**する。

- `MethodClass` を実測に見合うものへ
- `ActionKind` を `Persistent` へ
- **ロールバックを実測しろ。** 昇格の条件は「効く」だけでなく「**戻せる**」こと
  - 適用 → 別プロセスで観測 → 戻す → 再観測、を Action の経路で通せ
  - `docs/RULES.md` の「第三者の変更を上書きしない」を守れ
- 値が未設定（`original_reg=None`）の場合の復元を確かめろ。
  **既定値を書くな。値が無かったなら、無い状態へ戻せ**

## 手で揃える表を全部直せ

`docs/RULES.md` の「手で揃える表について」に一覧がある。**全部やれ。**
件数テスト、README の内訳表、CHANGELOG も。

## 検証

- 適用と復元を**別プロセスの観測**で確かめた `EVIDENCE:` 行を貼れ
- 値が未設定だった場合の復元も測れ
- `request_round_trip` `category_contract` `count_report` を通せ

## 完了条件

- `cargo test --lib` が全部通る
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（テスト通過、CSS警告0件）
- `docs/DISPLAY_ONLY_TRIAGE.md` と `docs/STATUS.md` を更新しろ
