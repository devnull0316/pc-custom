# Totonoe — 現況と再開手順（CC記録 2026-07-24）

## 到達点（検証済み）
- **設計**: docs/ に10文書 + DESIGN_LANGUAGE。CC監査 `CC_AUDIT_TASK1.md` 合格。技術構成=Tauri2+Rust（採点 A88.5/B76.5）。
- **基盤(Task2)＋ゲームプロファイル核(Task3)**: **実Windowsで `cargo test --lib` = 48 passed / 0 failed**。
- **ゲームプロファイル核(Task3, CC実装)**: `game_profile/` に OS/DB非依存の状態機械。
  - `ProfileSupervisor`: 起動→適用 / 終了→「最後の所有者になったresourceだけ」逆順復元
  - resource_key lease 共有(同一desired)＋競合停止(反対desired)、instance-key多重適用防止、AbortProfile/SkipConflicting方針
  - `ProcessMatcher`: canonical path＋file identity両一致でのみ検知（名前追従なし・identity不明は非適用）、PID再利用対応
  - 注入シーム `ProfileActionSink`/`ObservedProcess`。受入試験§11の核心を15テストで検証。
  - レジストリ正確復元（欠如↔欠如・raw無損失・第三者非上書き・kill-point再開）
  - トランザクション逆順rollback / 適用途中killのreconcile / 未知ビルドfail-closed
  - IPC攻撃spike（昇格Action全拒否・nonce replay/deadline・過大payload・PID再利用/署名不一致fail-closed）
  - 実Action6: session.prevent_sleep / power.active_scheme_check / explorer.show_extensions / explorer.show_hidden / theme.color_mode / games.process_watch
- **UI**: dev描画確認。結果ファーストのホーム、標準ユーザー表示、Ctrl+Kパレット、backend無しは`CORE_UNAVAILABLE`へ優雅に縮退、Segoe UI Variable/ティール#2fa6a0/Mica半透明/明暗/フォーカスリング。コンソールエラー0。

## いま出せていないもの（未実施）
- ゲームプロファイルの**実配線**: `ProfileActionSink`をTotonoeEngineへ、`ObservedProcess`供給をwindows/process.rs+wmi_process.rsのハンドル待ちへ結線。核ロジックは完成・テスト済みだが、実OS監視ループと実適用の結線が未。
- プロファイルの**Tauriコマンド＋UI**（作成/一覧/有効化/実行中状態、準備チェックpreflight）。
- フルの `cargo build`（binリンク）と `tauri dev` 実アプリ起動確認。
- 実機での実UI適用（theme切替を本番HKCUで適用→目視→rollback）。テストは隔離キー/FakeSinkのみ。
- A/B常駐メモリ実測、追加Action、data-only共有、AI候補、実験モジュール。

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
1. ゲームプロファイル実配線: `ProfileActionSink`→engine(preview/commit/rollback_item)、`ObservedProcess`供給→Toolhelp/WMI+handle待ち（windows/process.rs, wmi_process.rs流用）。ポーラ+補正snapshot。
2. プロファイルTauriコマンド＋UI（作成/一覧/有効化/実行中状態/準備チェック）。DESIGN_LANGUAGE準拠。
3. `cargo build` binリンク→`tauri dev`実起動→ホーム/カード/適用プレビュー/タイムライン/プロファイルの実操作。
4. theme.color_mode を実HKCUで apply→目視→rollback の実機縦切り（本番・事前バックアップ確認）。
5. A/B常駐メモリ実測（scripts/measure-private-working-set.ps1）。
6. 7/30 Codex復帰後: 残りをcodex実装 → CC監査（`CC_REVIEW.md`, 差し戻し1回まで）。追加Action/共有/AI/実験モジュール。

## オーケストレーション記録
Codex(実装) × CC(設計統括/監査/実ビルド検証) ループ。ドライバ: `../claude-codex-orchestrator/scripts/codex-worker.sh "<task>" sol <workdir>`（SANDBOX=workspace-write, EFFORT=ultra, 通信オフ=コード生成のみ）。
