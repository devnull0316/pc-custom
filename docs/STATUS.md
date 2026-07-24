# Totonoe — 現況と再開手順（CC記録 2026-07-24）

## 到達点（検証済み）
- **設計**: docs/ に10文書 + DESIGN_LANGUAGE。CC監査 `CC_AUDIT_TASK1.md` 合格。技術構成=Tauri2+Rust（採点 A88.5/B76.5）。
- **基盤(Task2)**: Rust約10k行 + React/TS。**実Windowsで `cargo test --lib` = 33 passed / 0 failed**。
  - レジストリ正確復元（欠如↔欠如・raw無損失・第三者非上書き・kill-point再開）
  - トランザクション逆順rollback / 適用途中killのreconcile / 未知ビルドfail-closed
  - IPC攻撃spike（昇格Action全拒否・nonce replay/deadline・過大payload・PID再利用/署名不一致fail-closed）
  - 実Action6: session.prevent_sleep / power.active_scheme_check / explorer.show_extensions / explorer.show_hidden / theme.color_mode / games.process_watch
- **UI**: dev描画確認。結果ファーストのホーム、標準ユーザー表示、Ctrl+Kパレット、backend無しは`CORE_UNAVAILABLE`へ優雅に縮退、Segoe UI Variable/ティール#2fa6a0/Mica半透明/明暗/フォーカスリング。コンソールエラー0。

## いま出せていないもの（未実施）
- フルの `cargo build`（binリンク）と `tauri dev` での実アプリ起動確認（webviewネイティブ窓）。
- 実機での実UI適用（例: theme切替を本番HKCUで適用→目視→rollback）。テストは隔離キーのみ。
- A/B常駐メモリ実測（scripts/ に手順あり、未計測）。
- Task3以降（ゲームプロファイル状態機械 / 追加Action / data-only共有 / AI候補 / 実験モジュール）。

## ブロッカー
- **Codex(sol/ultra)がusage limit到達。復帰 2026-07-30 05:56。** それまで実装はCC側で最小限に留める方針（ユーザ指示: Claudeのリミットを温存）。

## 再開コマンド（実機・ネットワーク要）
```
# 環境: rustc/cargo 1.97(MSVC) + VS BuildTools 2022(VCTools+Win11 SDK) 導入済み
export PATH="$USERPROFILE/.cargo/bin:$PATH"
cd src-tauri && cargo test --lib -- --test-threads=1     # 33 passed を再確認
cargo build                                              # binリンク（初回は数分）
cd .. && npm run dev                                     # UI（preview: totonoe-web / port1420）
npx tauri dev                                            # 実アプリ窓（要 cargo build 成功）
```

## 次にやること（優先順）
1. `cargo build` binリンク確認 → `tauri dev` 実起動 → ホーム/カード/適用プレビュー/タイムラインの実操作。
2. theme.color_mode を実HKCUで apply→目視→rollback の実機縦切り検証（隔離でなく本番、事前バックアップ確認）。
3. A/B常駐メモリ実測（scripts/measure-private-working-set.ps1）。
4. 7/30 Codex復帰後: Task3ゲームプロファイルを codex実装 → CC監査（`CC_REVIEW.md`, 差し戻し1回まで）。

## オーケストレーション記録
Codex(実装) × CC(設計統括/監査/実ビルド検証) ループ。ドライバ: `../claude-codex-orchestrator/scripts/codex-worker.sh "<task>" sol <workdir>`（SANDBOX=workspace-write, EFFORT=ultra, 通信オフ=コード生成のみ）。
