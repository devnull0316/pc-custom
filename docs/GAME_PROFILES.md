# ゲームプロファイル設計

## 1. 目的と安全境界

ゲームプロファイルは、登録したゲームの実行中だけ Action の組合せを適用し、そのゲームのために変更した項目だけを終了時に元へ戻す機能である。FPS向上を保証する機能ではなく、通知、スリープ、アプリ起動、見た目などの準備と後片付けを再現可能にする。

自動化対象は登録済み Action と schema 検証済みparameterに限る。ゲームへのDLL injection、memory scan、debug privilege、kernel driver、anti-cheat回避、未知のlauncherを推測して追跡する処理は行わない。

## 2. プロファイルのデータモデル

| 項目 | 内容 |
| --- | --- |
| `profileId` | 変更されない内部ID |
| `name`, `description` | ユーザー表示用。実行内容ではない |
| `binding` | 実行ファイルのcanonical path、file identity、publisher/hashの任意補助情報、追跡方式 |
| `actions` | 順序付きの登録済みAction IDとschema適合parameter |
| `launchSet` | 端末上でユーザーが明示登録したopaque app IDの集合。MVPは引数なし。path/argvは共有・AI・profile本体に含めない |
| `conflictPolicy` | 競合時に新規適用を止める方針。自動上書きは禁止 |
| `compatibility` | profile schema版、必要Action版、対応アプリ版 |
| `automationEnabled` | 実行ファイル再確認と互換性検査を通った場合だけ有効 |
| `lastValidatedAt` | bindingとAction互換性を最後に検証した時刻 |

実行ファイル登録時は、ユーザーが選んだ絶対パスをcanonicalizeし、通常ファイルであること、実行可能形式であること、許可されたlocal volume上であることを検証する。reparse point、ネットワークpath、曖昧な相対pathは自動適用の対象外とする。ファイルが更新・移動されidentityが変わった場合は、名前だけで追従せず「再確認が必要」としてautomationを止める。

起動appのlocal登録ではknown shell/script host、system binary launcher、file associationを拒否し、MVPではargvを受けない。profileは既存local app IDを参照するだけで、新しい実行対象を作れない。これにより共有/AIから任意code実行へ迂回できないようにする。

## 3. 状態機械

基本遷移は BRIEF §9 をそのまま満たす。

`登録済み → 待機 → 起動検知 → 適用中 → プレイ中 → 終了検知 → 復元中 → 待機`

永続化するruntime状態は次のとおりである。

| 状態 | entryで行うこと | exit条件 | durableに保存する情報 | 異常時の扱い |
| --- | --- | --- | --- | --- |
| `registered` | bindingとActionを検証 | ユーザーがautomationを有効化 | profile版、binding検証結果 | 不一致なら`needs_rebind` |
| `idle` | 対象が未実行であることを確認 | 同一性を満たすprocess start | 最終観測時刻 | イベント欠落は補正snapshotで回復 |
| `detected` | PIDだけでなくpath、creation time、可能ならfile identityを確認 | instance確定 | instance key、検出根拠 | access deniedは`unknown`、適用しない |
| `applying` | transaction IDを作り、全Actionを事前検証・backup・順次適用 | 全verify成功 | journal、backup、resource lease | 失敗時は逆順rollback |
| `playing` | process handleの終了待ちと低頻度補正 | 最後の対象instance終了 | active instance一覧、lease owner | core再起動時はrecovery判定用に観測を再構築するが、旧sessionを暗黙再開しない |
| `ending` | 終了を再確認し、複数instanceを集約 | profileの対象が0件 | 終了根拠 | 一時的観測欠落では復元しない |
| `restoring` | このsessionが取得したleaseを逆順release | rollback全件検証 | 各itemの復元結果 | 失敗は`recovery_required` |
| `recovery_required` | 自動復旧可能性を評価 | rollback成功またはユーザー判断 | 未復元item、error、retry回数 | 新規自動適用より優先 |
| `needs_rebind` | automation停止 | ユーザーが実行ファイルを再確認 | 変更前後のbinding情報 | 勝手に別EXEへ追従しない |

状態遷移はUIの表示ではなくSQLite journalを正とする。`applying`へ入る前と各Actionの境界でdurable commitし、OS変更とjournalが不整合になり得る短い区間をreconciliationで判定できるよう、backup、期待適用値、検出結果を残す。

## 4. 起動・終了検知

### 4.1 起動検知

MVPは次の組合せを使う。

1. core起動時にToolhelpまたはWMIでprocess snapshotを取り、既に起動中の対象を発見する。
2. `Win32_ProcessStartTrace` / `Win32_ProcessStopTrace` 相当のWMI process traceで低遅延イベントを受ける。
3. イベントのPIDから、取得できる場合は限定権限でprocess handleを開き、`QueryFullProcessImageName`相当で完全pathを確認する。
4. PID再利用を避けるため、PIDとcreation timeをinstance keyにする。
5. 低頻度snapshotを補正経路とし、イベント欠落やobserver再接続を回復する。

WMIイベントの名前やPIDだけでは本人性の根拠にしない。権限差や保護processでpath確認ができない場合は`状態不明`と表示し、自動適用しない。

### 4.2 終了検知

path確認後に取得したprocess handleへ待機を登録し、そのhandleがsignaledになったことを主な終了根拠とする。WMI stop eventと補正snapshotは補助である。同名processの消失だけでは別instanceとの取り違えがあるため、instance key単位で管理する。

### 4.3 launcherと子process

MVPは次の二方式だけを明示選択させる。

- `exact_executable`: 登録したEXEの各instanceが存在する間をプレイ中とする。
- `explicit_process_group`: ユーザーが個別に登録したlauncher/game EXE集合のうち、開始条件と終了条件を設定する。

親子関係、window title、短時間で消えるlauncherから本体を自動推測する方式は誤検出しやすいためMVPでは採用しない。Steam等のURIは起動導線には使えても、終了判定は確認済みEXEに結び付ける。

## 5. 多重適用防止と資源リース

同じActionをprofileごとに独立backupすると、終了順によって誤った値へ戻る。そのため、変更対象をcanonicalな`resourceKey`へ解決し、core全体でleaseを管理する。

| 条件 | 判断 |
| --- | --- |
| 同じprofileの同じinstance startを重複受信 | instance keyで重複排除し、再適用しない |
| 同じprofileの別instanceが開始 | profile owner数を増やす。Actionは再適用しない |
| 別profileが同じresourceへ同じdesired stateを要求 | 共有leaseを追加。最初のownerのbackupを共有する |
| 別profileが同じresourceへ反対のdesired stateを要求 | 後から来たprofileの該当Actionを競合として止め、先行状態を維持する |
| 一方のprofileが終了 | そのownerだけrelease。ownerが残る間は復元しない |
| 最後のownerが終了 | 最初の適用前backupへ一度だけrollbackする |

競合のため一部Actionを適用できない場合、profile全体を事前定義したtransactionとして扱い、デフォルトではprofileの自動適用を中止する。ユーザーが明示的に許可した「任意Action」だけはskip可能だが、その判定もjournalへ残す。

resource lockとleaseはDBだけでなく単一core process内の排他制御にも結び付ける。異なるAction IDでも同じregistry valueや同じpower settingへ到達する場合、互換性registryで同じresourceKeyへ正規化する。

## 6. 適用と復元の順序

1. 対象instanceの本人性を確定する。
2. profile、Action、Windows build、現在状態、conflict/dependencyを全件検証する。
3. 変更対象ごとにlockを取り、必要なleaseを予約する。
4. 新規resourceだけlossless backupをdurable保存する。
5. Actionを宣言順に適用し、各`verifyApplied()`を通す。
6. profile sessionを`playing`へcommitする。
7. 最後の対象instance終了後、取得したleaseをAction適用の逆順にreleaseする。
8. 最終ownerになったresourceだけrollbackし、`verifyRolledBack()`を通す。

途中失敗時は、そのsessionが新規に変更したActionだけを逆順rollbackする。既存ownerと共有したresourceはreleaseだけ行い、他profileの状態を壊さない。rollback失敗は成功扱いにせず、復旧画面と変更タイムラインへ残す。

## 7. クラッシュと再起動からの復旧

### 7.1 PCカスタムがクラッシュした場合

次回core起動時は、新しいprofile監視より先に未完了journalを走査する。

- `applying`: backup、期待適用状態、現状態を照合し、適用済み分を逆順rollbackする。
- `playing`: ゲームがまだ動作中でも、前sessionの監視保証を失ったため安全側として一度rollbackする。ユーザー設定が許す場合だけ、復旧完了後に新sessionとして再適用する。
- `restoring`: 未検証itemを再検出し、適用fingerprintのままならrollbackを再試行する。
- 現状態がユーザーや別アプリにより変わっている場合は自動上書きせず、`競合・要判断`にする。

OS再起動や電源断でも同じjournalを用いる。自動復旧は有限回で止め、永久loopにしない。

### 7.2 ゲームがクラッシュした場合

process handleの終了は正常終了とクラッシュを区別せず検知できるため、最後のinstanceが消えたら通常と同じ復元へ進む。ゲームのexit codeを取得できる場合は診断情報に使うが、復元可否の条件にはしない。

### 7.3 suspend、sleep、logoff

- system sleep前に新規適用を始めず、進行中journalをflushする。
- resume後はprocess snapshotとleaseを再照合してから監視を再開する。
- user logoff/shutdown通知ではbest-effortで復元を試みるが、時間制限に依存しない。未完了なら次回起動時recoveryが責任を持つ。
- appの通常終了要求は、active profileがあれば復元結果を示してからcoreを終了する。

## 8. 複数ゲームと手動操作

複数ゲーム同時起動は拒否しない。共有可能なdesired stateは参照カウントし、反対要求は先行優先で後発を明示停止する。優先度で勝手に上書きする方式は、終了順と復元点を分かりにくくするため採用しない。

ユーザーがプレイ中にWindows設定を手動変更した場合、rollback前の検出結果が適用fingerprintと異なる。PCカスタムはその変更を黙って消さず、次の選択肢を表示する。

1. 現在値を維持し、leaseを`外部変更あり`として閉じる。
2. backupの具体的内容を確認し、現在の第三状態を新たにbackupした別transactionとしてpreview・明示確認後に元へ戻す。
3. 後で判断するため復旧項目として残す。

「今すぐ停止」は監視だけを捨てる操作にしない。active profileを復元してから停止する`安全に停止`と、復元失敗を残したまま強制終了する診断用操作を分離する。

## 9. UIで伝えること

profileカードと実行中画面には以下を表示する。

- 対象EXEと最後に確認したidentity
- 現在の状態（待機、確認中、適用中、プレイ中、復元中、復旧必要）
- 適用予定/適用済み/skip/失敗Action
- 共有中または競合中のresourceと他profile名
- 変更前と適用後の状態、危険度、admin・再起動・Update影響
- 「終了を検知したら戻す」対象と、最後に復元を検証した時刻

ゲームが速くなった、遅延が減った等は計測なしに表示しない。説明は「この準備を自動化する」「この設定を一時的に変更する」とする。

## 10. MVPと後回し

### MVP

- exact executableの登録と再確認
- 初期snapshot + WMI event + process handle wait +補正snapshot
- 単一/複数instance、複数profileのresource lease
- 標準権限Actionによるtransaction適用・復元
- app/game crash後のrecovery-first起動
- 状態と競合を説明するUI

### 後回し・実験的

- ETWによる高速process追跡: profiler/権限/互換性を実測後に検討
- 自動launcher子process推測: 誤検出率を評価できるまで手動登録で代替
- protected processや別ユーザーsessionの追跡: 権限境界を越えない
- 表示Hz/HDR/既定audioの自動切替: 公開契約・hardware matrix・正確なrollbackが揃うまでread-only
- process priority、CPU affinity、service停止、network stack変更: 効果と副作用が大きくMVP対象外

実験的機能はstable Action registry、main helper、stable backup schemaへ直接追加しない。別署名のexperimental host、別allowlist、別database namespace、明示opt-in、kill switch、build単位の自動無効化を要求する。

## 11. 受入試験

1. 同一start eventの重複、PID再利用、path不一致で多重適用しない。
2. ゲームが既に起動中の状態でPCカスタムを起動しても正しく検出する。
3. 同じprofileの2instanceでは最初に1回だけ適用し、最後の終了時に1回だけ復元する。
4. 同じdesired stateを要求する2profileは共有し、最後の終了まで状態を維持する。
5. 反対状態を要求する2profileでは後発を適用せず、理由を表示する。
6. Action途中失敗、core強制終了、OS再起動の各点から元の欠如/型/値へ復元できる。
7. プレイ中の手動変更を検知し、黙って上書きしない。
8. 実行ファイル更新・移動・reparse化でautomationが停止する。
9. WMI observer断をsnapshotで補正し、同じinstanceを再適用しない。
10. protected/access denied対象では`不明`を`未実行`と誤判定しない。

## 12. 一次情報候補と確認状態

- Microsoft Learn, `CreateToolhelp32Snapshot`: <https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot>
- Microsoft Learn, `QueryFullProcessImageNameW`: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew>
- Microsoft Learn, `RegisterWaitForSingleObject`: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-registerwaitforsingleobject>
- Microsoft Learn, `Win32_ProcessStartTrace`: <https://learn.microsoft.com/en-us/previous-versions/windows/desktop/krnlprov/win32-processstarttrace>

APIの存在と基本契約は一次情報で確認した。WMI eventとhandle waitを組み合わせたときの欠落率、保護process、各game launcher、sleep/resumeでの挙動は実機未検証であり、Task 2/3で`未検証・要CC確認`としてtest matrixを消化する。
