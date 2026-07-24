# Totonoe — 現況と再開手順（CC記録 2026-07-24）

## 到達点（検証済み）
- **設計**: docs/ に10文書 + DESIGN_LANGUAGE。CC監査 `CC_AUDIT_TASK1.md` 合格。技術構成=Tauri2+Rust（採点 A88.5/B76.5）。
- **基盤(Task2)＋ゲームプロファイル核(Task3)**: **実Windowsで `cargo test --lib` = 48 passed / 0 failed**。
- **ゲームプロファイル(Task3, CC実装 — 端から端まで)**:
  - 核 `game_profile/`: `ProfileSupervisor`(起動→適用/終了→最後の所有者のresourceだけ逆順復元, lease共有/競合停止/多重適用防止, AbortProfile/SkipConflicting)、`ProcessMatcher`(path＋identity両一致検知, identity不明は非適用, PID再利用対応)。注入シーム`ProfileActionSink`/`ObservedProcess`。
  - 永続化 `ProfileStore`: JSON原子的保存(安全journalと分離)、`registered_file_identity`で実行ファイル検証(ローカル固定ボリューム/非reparse/本人性)、未知Action ID拒否、既定は自動適用オフ。
  - Tauriコマンド: profiles_list/create/set_enabled/delete、ApplicationStateへ配線。
  - **UI**(`ProfilesView`): 作成フォーム/一覧/有効化トグル/削除、結果志向・誇張なし・a11y、ブラウザペインで描画確認・console 0。
  - テスト: 受入試験§11の核心＋matcher＋store＝**51 passed**。
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
残るのは**2つの薄いI/Oアダプタ＋起動配線**のみ（論理は`ProfileRuntime`まで完成・テスト済み）:
1. 実`ProfileActionSink`(engine背面): action集合を適用しper-item復元参照を返す。engineに per-item id を返すapply経路を1つ追加(現状commit_previewはtransaction_idのみ)＋既存rollback_item流用。**実HKCUを触るため実機縦切り検証必須**。
2. 実`ObservedProcess`供給: windows/process.rs `snapshot_process_identities`＋wmi_process をポーラ(例1〜2秒間隔+補正)で`ProfileRuntime.tick`へ。背景スレッド。
3. `ProfileRuntime`をApplicationStateへ配線し、プロファイル有効化で起動。
4. `cargo build` binリンク→`tauri dev`実起動→全画面の実操作。
5. theme.color_mode を実HKCUで apply→目視→rollback の実機縦切り。A/B常駐メモリ実測。
6. 7/30 Codex復帰後: 追加Action/準備チェックpreflight/data-only共有/AI候補/実験モジュール → CC監査。

## オーケストレーション記録
Codex(実装) × CC(設計統括/監査/実ビルド検証) ループ。ドライバ: `../claude-codex-orchestrator/scripts/codex-worker.sh "<task>" sol <workdir>`（SANDBOX=workspace-write, EFFORT=ultra, 通信オフ=コード生成のみ）。
