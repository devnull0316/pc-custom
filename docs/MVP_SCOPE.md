# MVP スコープ

## 1. スコープ方針

MVP は「多機能」より、変更の検出・保存・適用・検証・復元という一連の信頼性を完成させる。Windows 11 の公開 API、公式 CLI、または Microsoft が文書化した設定を優先し、書き込み契約が明確でない項目は Action の型だけを用意しても、自動適用を互換性ゲートで止める。

2026-07-24 時点の対象候補は、サポート期間内の Windows 11 24H2、25H2、26H1 である。ただし「Microsoft がサポート中」と「Totonoe が試験済み」は別であり、実機試験が終わるまでは `未検証・要CC確認` と表示する。26H1 は既存 24H2/25H2 機への通常の機能更新ではないため、独立した試験行として扱う。

## 2. MVP に入れるもの

### P0: 安全基盤

- Action レジストリ、厳密な状態検出、型付きパラメータ
- 変更前状態を保存してから適用するトランザクション
- 逆順 rollback、rollback 検証、クラッシュ後の未復元検知
- 変更タイムラインと、1件だけ・指定時点までの復元
- Windows ビルド差異の集中管理と、未知ビルドでの安全停止
- 標準権限 UI/コアと短命な昇格ヘルパーの境界
- ローカル JSON の安全なプロファイル入出力。任意コードは不可

### P1: 初期 Action

14 Action を設計対象とする。うち、Microsoft の公開書き込み契約が確認できないシェル設定は、Task 2 の実機試験を通るまで「検出・説明・手動案内」のみとし、自動適用を有効にしない。

### P2: ゲームプロファイル

- ローカル実行ファイルとプロファイルの紐付け
- 標準権限での起動・終了検知
- Action の一時適用、所有権付き lease、終了時の逆順復元
- 同一 Action の多重適用防止
- 複数ゲーム同時起動時の同値共有と競合停止
- Totonoe またはゲームのクラッシュ後の復旧

## 3. 初期実装 Action 14件

危険度は `安全 / 注意 / 実験的` の3段階である。「安全」は絶対安全ではなく、変更範囲が限定され、状態検出と正確な復元を設計できるという意味で使う。

| ID / 結果表示 | 変更手段 | 保存する復元データ | rollback 手順 | 危険度 | admin | 再起動 | Windows Update 影響 | 状態検出 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `explorer.show_extensions` / ファイルの拡張子を常に表示する | HKCU Explorer 設定 `HideFileExt` の限定レジストリ操作。値の実書込契約は一次資料で未確定のため実機ゲート | キー有無、値有無、型、元の生データ、適用値、レジストリ view | 適用値と一致することを確認し、元値を型ごと復元。元々無ければ値を削除。Explorer へ設定変更通知 | 安全。ただし自動適用は検証後 | 不要 | OS不要。Explorer再読込要否はビルド試験 | 中。Shell 更新で再検証 | レジストリは可。画面反映は要試験 |
| `explorer.show_hidden` / 隠しファイルを表示する | HKCU Explorer 設定 `Hidden` の限定レジストリ操作。保護されたOSファイル表示は対象外 | 同上 | 同上 | 注意。誤操作しやすいファイルが見える | 不要 | OS不要。Explorer再読込要否は要試験 | 中 | レジストリは可。画面反映は要試験 |
| `taskbar.widgets_visibility` / ウィジェットボタンを表示・非表示にする | Windows 11 設定資料で示される taskbar/Widgets のレジストリ状態。実際の書込値・型をビルド別 fixture で確定後のみ自動化 | 対象キー/値の有無、型、生データ、適用値 | 比較後に元状態へ復元。必要時のみ Explorer へ通知し、強制終了はしない | 安全。ただし書込契約は未検証 | 不要 | OS不要。Explorer再読込は要試験 | 中〜高。Taskbar変更の影響を受けやすい | 候補値は可。表示との一致は要試験 |
| `taskbar.clock_seconds` / タスクバー時計に秒を表示する | 設定 UI は存在するが、安定した公開変更 API を確認できていない。MVP初期は手動案内、レジストリ自動化は無効 | 自動化を有効にする場合のみ完全なレジストリ snapshot | 手動案内のみなら no-op。自動化は元の欠如状態まで復元 | 注意 | 不要 | OS不要の想定だが未検証 | 高。Shell更新の影響大 | `未検証・要CC確認` |
| `theme.color_mode` / Windows とアプリをダーク・ライト表示にする | Microsoftの設定状態資料にあるHKCU `Themes\Personalize`の2値を候補とする。third-party write契約は未確認で、初期はdetect/guided。24H2/25H2/26H1、contrast theme、policyのgate後だけ自動化 | 2値それぞれのキー/値有無、型、元rawデータ、適用値、configured/effective観測 | 2値を逆順復元し通知。片方失敗時もtransaction rollback。contrast/policy/第三状態では自動writeしない | 注意。自動適用は検証まで無効 | 不要 | OS不要。一部app再起動 | 中 | configuredは可。effective反映は部分的 |
| `theme.transparency` / Windows の透明効果を切り替える | Microsoftの設定状態資料にあるHKCU `Themes\Personalize\EnableTransparency`を候補とする。write契約は未確認で、初期はdetect/guided | 完全registry snapshot、configured/effective観測、contrast/policy状態 | 元値または元の欠如状態へ復元。contrast/policy/第三状態では停止 | 注意。自動適用は検証まで無効 | 不要 | OS不要 | 中 | configuredは可。effectiveは要試験 |
| `gaming.game_mode` / Windows のゲームモードを切り替える | Microsoft が状態参照用に記載する HKCU `Software\Microsoft\GameBar\AutoGameModeEnabled`。第三者の書込契約は未確認 | 完全なレジストリ snapshot | 元状態へ復元し、次回ゲーム起動前に検証 | 注意。自動適用は実機検証まで無効。性能向上は保証しない | 不要 | 反映条件は未検証 | 中〜高 | 設定値は可。実効状態とは分ける |
| `session.prevent_sleep` / このモード中は自動スリープを防ぐ | 公開 Win32 API `SetThreadExecutionState`。画面常時点灯は別パラメータで既定OFF | lease 所有者、開始時刻、使用フラグ、API結果、実行スレッド識別子 | 最後の lease 解放時に `ES_CONTINUOUS` で要求を解除。プロセス終了時はOSが自動解除 | 安全 | 不要 | 不要 | 低 | Totonoe の lease は可。OS全体の全要求一覧ではない |
| `apps.launch_set` / 必要なアプリをまとめて起動する | 端末上でユーザーが明示登録したopaque app IDだけを参照。MVPは引数なし、shell/file associationなし、known script host/LOLBins拒否。検証済み絶対EXEを`CreateProcessW`のapplication nameへ明示し、handle継承を無効化 | 起動前に存在したprocess、作成PID・時刻・file identity、local registration ID、終了方針 | 既定は追跡解除のみ。明示的`closeOnRollback`時だけ同一processを確認して通常終了要求。強制終了しない | 注意。未保存dataの可能性 | 不要 | 不要 | 低 | 可。ただしprotected processは不明 |
| `games.process_watch` / 登録したゲームの起動・終了を検知する | 公開 Tool Help API のプロセス snapshot と `QueryFullProcessImageName`。注入・ゲーム改変なし | 登録ファイルID、正規化パス、検知PID・作成時刻、監視世代 | 監視登録と lease を解除。OS変更なし | 安全 | 不要 | 不要 | 低 | 可。アクセス拒否時は不明 |
| `startup.inventory` / 自動起動するアプリを確認する | baselineは標準権限で読めるHKCU Run/RunOnceとuser Startup folder。WMI/HKLM/common sourceはprivilege差があるためbest-effort | 読取時刻、情報源、取得可否、正規化した項目。復元用変更dataなし | no-op。MVPでは無効化・削除しない | 安全 | baseline不要。追加sourceは制限あり | 不要 | 中。起動元追加に追従が必要 | 部分的。sourceごとのunknownを表示し、網羅を主張しない |
| `power.user_mode` / 省電力・バランス・高パフォーマンスの希望を切り替える | Windows 11 公開 Win32 Power API。AC/DC を別々に設定 | AC/DCそれぞれの元GUID、適用GUID、実効モード観測値 | 現在のconfigured値が適用値ならAC/DCを各元GUIDへ復元。外部変更なら競合停止 | 注意。バッテリー消費・発熱が変わり得る | 原則不要。標準ユーザー実機は要確認 | 不要 | 低〜中。OEM/policyが実効値を上書きし得る | configured値は可。effective値も別APIで可 |
| `power.active_scheme_check` / 現在の電源設定を確認する | 公開 Power API、必要時に Microsoft 公式 `powercfg` の読取専用サブコマンド | 読取結果と時刻のみ | no-op | 安全 | 不要 | 不要 | 低 | 可。OEM独自モードは不明の場合あり |
| `games.readiness_check` / ゲーム前の準備漏れを確認する | 上記の状態検出、OS build、音声/表示の読取専用公開APIを合成。変更は行わない | 各 probe の値、取得時刻、根拠、`unknown` 理由 | no-op | 安全 | 不要 | 不要 | 中。probe互換性を再評価 | probeごとに可/一部/不明 |

### Action の出荷ゲート

次をすべて満たさない Action は「初期カタログに表示」できても「自動適用可能」にはしない。

1. 変更手段が BRIEF の優先順位に合う。
2. 対象 build の実機で状態検出、適用、再検出、rollback、再検出が成功する。
3. 値が元々無い場合、異なる型の場合、ポリシー管理下の場合を試験する。
4. rollback 前に外部変更を検出できる。
5. Explorer またはアプリの再起動条件を説明できる。
6. `maximumTestedBuild` と試験証跡が互換性カタログに登録される。

## 4. MVP に入れないもの

### 永久に禁止

- Microsoft Defender、Windows Firewall の無効化
- Windows Update の完全停止
- ページファイル無効化、HPET/BCD の安易な変更
- 根拠の薄い「FPS向上」レジストリ
- 大量サービスの一括停止
- ゲーム/アンチチートへの注入、ゲームプロセス・ゲームファイル改変
- 任意 PowerShell、cmd、bat、DLL、EXE、JS、reg、任意シェルコマンドの共有・実行
- 出所不明スクリプト/バイナリの取得・実行

### 後続の実験的モジュールへ分離

- 通知の「応答不可/集中モード」を外部アプリから強制切替
- モニターリフレッシュレートの自動変更
- HDR の自動切替
- 既定オーディオデバイスの自動切替
- 電源プラン/電源モードの自動変更
- Explorer の強制終了・再起動
- WinGet/App Installer の自動導入
- スタートアップ項目の無効化・削除
- タスクスケジューラへの登録
- ETW を使う低遅延プロセス監視
- PowerToys 等、外部ツールの自動設定変更

ここで後回しにする電源機能は、ゲーム開始等の条件で自動適用するもの、従来のpower scheme全体を変更するものを指す。公開Windows 11 APIを使いユーザーがpreview後に一回だけ明示変更する`power.user_mode`はMVP候補のままである。

分離方法は [ARCHITECTURE.md](./ARCHITECTURE.md) の「実験的モジュール」と [SECURITY.md](./SECURITY.md) に定める。

## 5. PowerToys 等との境界

PowerToys と同等の機能は、再実装を既定にしない。

1. Windows/PowerToys の公式設定画面への案内
2. WinGet が既に利用可能な場合の導入支援
3. 公開された公式設定連携がある場合のみ、その必要部分を型付き Action 化
4. 公式な連携手段がない場合は自動変更しない

## 6. MVP 完了の定義

- 上表のうち、自動適用として出荷する全 Action が出荷ゲートを通っている。
- 対応 build ごとの試験結果が `WINDOWS_COMPATIBILITY.md` のモデルで管理される。
- 一括適用の各永続化境界で故障注入試験が成功する。
- 未復元セッションが次回起動時に検出され、競合を隠さず復元できる。
- 権限付き処理が Action ID と型付き引数以外を受け付けない。
- 禁止機能をプロファイル、AI、将来プラグインから迂回できない。
