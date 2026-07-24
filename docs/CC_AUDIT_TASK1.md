# CC独立監査 — Task 1（設計フェーズ）

基準: `../claude-codex-orchestrator/CC_REVIEW.md`。設計フェーズのため対象は「要件網羅・高リスクWindows仕様の妥当性・文書整合・禁止事項の混入」。コードは無い。

## 総評
**重大な問題なし。Task 2 へ GO（条件付き）。** 12項目網羅、MVP14 Action（10-15内）、禁止リスト遵守、実験機能の隔離設計あり。特に「未検証の書込機構を自動化せず guided/read-only へ落とす」保守姿勢は正しい。rollbackの状態分類（original/applied/third/unknown）とunknown-build fail-closed（RECOVERY_REQUIRED）は堅牢。

## 指摘

- **P2（正確性）** clock_seconds等でExplorerへ反映するには、Explorer強制再起動の前に非破壊のブロードキャスト（`SHChangeNotify(SHCNE_ASSOCCHANGED,...)` 等）で反映を試み、**失敗時のみ**Explorer再起動、を設計へ明記すること。Explorer強制再起動は実験扱いのため、通常Actionの既定反映手段はブロードキャストにする。→ `ACTION_SYSTEM.md` / `WINDOWS_COMPATIBILITY.md` に追記。
- **P3（改善・格上げ可）** 次はWin10/11で長年安定・文書化された標準HKCU setterであり、「未検証・要CC確認」ではなく**「安全・自動化可（実機スモーク＋反映ブロードキャストを条件）」**へ格上げしてよい（CC知見。実機スモークは残す）:
  - `Explorer\Advanced` の `HideFileExt`(0=表示) / `Hidden`(1=表示) / `ShowSecondsInSystemClock`(1=表示)
  - `Themes\Personalize` の `AppsUseLightTheme` / `SystemUsesLightTheme`(0=dark,1=light) / `EnableTransparency`
  - 一方、**taskbar検索/Widgets/Game Mode系はbuild揮発性が高い**（Win11の各ビルドでキー・意味が変動）ため guided 据え置きが妥当。CCの格上げ対象に含めない。
- **P3** DND/Focusの実験据え置きは妥当（24H2以降にクリーンな公開setterが無い）。第一手段は「設定アプリへ誘導するguided」にする。
- **P3** `apps.launch_set` の opaque app ID 起動は、known host/launcher拒否とファイル関連付け/LOLBins経由の任意実行防止の**境界をTask2でfuzz必須**。設計意図は正しい。

## 未検証の残存リスク（実機必須・設計では判断不能。Task2の攻撃/故障試験へ）
named pipe の exact SDDL/MIL、PID reuse、orphan privileged journal の reconcile、A/B同一負荷での常駐メモリ実測。→ codexの自己申告と一致。妥当。

## codexがCCに確認を求めた点への回答
- **技術構成A(Tauri2+Rust)採用: 承認。** ただしTask2のA/B常駐メモリ実測を最終ゲートにするのは同意。予算未達なら再採点。
- **install範囲**: **per-user版（helperなし・admin Action無し）を既定エディション**、machine-scope helperは admin Action を使う人向けの**オプトイン**、を推奨（最小権限）。
- **unknown-build rollback**: fail-closed(RECOVERY_REQUIRED) を承認。
- **配点 30/25/20/15/10**: 妥当。

## Task 2 の条件
上記P2を設計に反映し、P3の格上げ4項目＋taskbar系据え置きをMVP分類へ反映。Task2の縦切りは ORCHESTRATION_OUTPUT §12 の順で可。差し戻しは1回まで、未解決はユーザーへ。
