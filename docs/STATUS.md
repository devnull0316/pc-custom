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

## 実配線 完了（Task3ライブ）
- 実`EngineProfileSink`: 既存engine公開経路(preview→commit→list_timeline→rollback_item)だけで実適用/復元。engine無改修。per-item参照でlease共有下も正しく復元。
- 実`ObservedProcess`供給: `snapshot_process_identities`をポーラ(3秒)で`ProfileRuntime.tick`へ。有効プロファイルが無い間はスナップショットも省く省コスト。
- `ProfileWatcher`背景スレッド → ApplicationStateへ配線(起動時spawn/Drop join)。クラッシュは次回reconcileが引き取る。
- **フルアプリ `totonoe.exe` ビルド成功**(17.8MB, main+generate_context+dist埋込)。
- **実機スモーク合格**(`#[ignore]`, `--ignored`で実行): 実HKCU HideFileExtに apply→値変化→rollback→**型・値・有無まで正確復元**。本番mutation経路を Windows 25H2(26200, TestedMutable)で実証。

## 残り
1. **インタラクティブGUI起動の目視**: このツール環境はデスクトップ/WebView2窓が無くGUI起動検証不可(exe即exit)。ユーザーが実デスクトップで `src-tauri/target/debug/totonoe.exe` か `npx tauri dev` を起動して確認。
2. A/B常駐メモリ実測（scripts/measure-private-working-set.ps1）。
3. 追加Action / 準備チェックpreflight / data-only共有 / AI候補 / 実験モジュール（7/30 Codex復帰後にcodex実装→CC監査 が省コスト）。

## オーケストレーション記録
Codex(実装) × CC(設計統括/監査/実ビルド検証) ループ。ドライバ: `../claude-codex-orchestrator/scripts/codex-worker.sh "<task>" sol <workdir>`（SANDBOX=workspace-write, EFFORT=ultra, 通信オフ=コード生成のみ）。
