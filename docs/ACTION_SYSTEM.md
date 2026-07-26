# Action システム

## 1. 設計原則

Action は「1つのユーザー結果」を検出・説明・適用・検証・復元する、PCカスタム の最小単位である。Windows API の1呼出しやレジストリの1値と常に同じではない。例えばダーク/ライト切替は、ユーザーにとって1つの結果なので複数値を1 Action の内部 transaction として扱う。

Action 実装はアプリに登録済みの first-party code のみである。プロファイルや AI は Action ID と schema に合うパラメータを選べるが、実装、コマンド、レジストリ path を追加できない。

## 2. Action 定義

BRIEF 必須フィールドをそのまま持つ。

| フィールド | 型・規則 |
| --- | --- |
| `id` | 安定したlowercase namespace。再利用・意味変更をしない |
| `name` | 結果を表す日本語名 |
| `description` | 対象者、非対象者、変更手段、制約 |
| `category`, `tags` | 固定enumと登録済みtag |
| `supportedWindowsVersions` | Windows 11 のrelease family。表示用で、build判定を代替しない |
| `minimumBuild` | 実行可能なbase build下限 |
| `maximumTestedBuild` | 実際に試験した上限。未来buildを推測しない |
| `riskLevel` | `安全`, `注意`, `実験的` |
| `requiresAdmin` | metadataだけでなくhandler側でも強制 |
| `requiresRestart` | OS restart |
| `requiresExplorerRestart` | Explorer再読込/再起動。MVPでは強制再起動を使わない |
| `conflicts` | 同時適用できないAction IDまたはresource/desired state条件 |
| `dependencies` | 事前に検出・適用が必要なAction ID |

安全実装のため次も登録する。

- `actionVersion`: apply/backup/rollback codec のversion
- `kind`: `persistent`, `session`, `observation`, `guided`
- `parameterSchema`: enum、範囲、必須/追加field拒否
- `resourceKeys`: 排他・leaseの単位
- `methodClass`: 公開API / 公式CLI / WinGet / 公式module / 文書化registry / 限定external / 未立証storage
- `evidenceUrls`: 一次資料
- `compatibilityKey`: 集中互換性表への参照
- `backupCodecVersion`, `rollbackDecoderVersions`
- `autoApplyEligible`: game profile で自動適用できるか

### Action kind

| kind | 例 | rollbackの意味 |
| --- | --- | --- |
| `persistent` | theme、power user mode | 保存した変更前状態へ戻す |
| `session` | sleep prevention、app launch | lease解除、PCカスタムが作ったsession資源だけを通常終了 |
| `observation` | startup inventory、readiness check | OS変更なし。観測sessionを閉じるno-op |
| `guided` | 公開setterがないDNDの案内、setter根拠未承認storageの読取候補 | 新しいOS変更を作らず、自動適用済みとは記録しない。旧durable backupだけは復旧可能 |

guided/observation も統一UIのため Action interface を実装するが、変更transactionの成功数や「復元可能な変更」に数えない。

### CC Task 1監査で確定したstable HKCU setter

`Explorer\Advanced`の`HideFileExt`、`Hidden`、`ShowSecondsInSystemClock`と、`Themes\Personalize`の`AppsUseLightTheme`、`SystemUsesLightTheme`、`EnableTransparency`は、CC監査P3により**安全・自動化可（対象buildの実機smokeと非破壊broadcastを条件）**へ格上げする。Task 2では`explorer.show_extensions`、`explorer.show_hidden`、`theme.color_mode`を前倒し実装する。clock secondsとtransparencyもguided固定ではなく同じstable分類だが、今回の縦切り対象外である。

taskbar検索、Game Modeを含む今回追加の固定HKCU候補42件は、この格上げに含めない。参照したSettings schemaや概説ページはWindows UI setter契約を立証せず、対象buildの実機UI round-tripも未実施である。そのためIDとparameter schemaはcatalogへ保持するが、`kind=guided`、`methodClass=unverified_storage`、`riskLevel=experimental`、`autoApplyEligible=false`とする。検出は固定DWORDの保存値だけを表示し、有効なWindows UI状態とは解釈しない。`validate`、`createBackup`、`apply`はhandler自身が`setter_evidence_pending`で拒否し、compatibilityの一般判定だけに依存しない。CCがAction固有の一次資料と24H2/25H2実機試験を承認するまでrelease buildで書込み経路を持たない。Widgetsは既存の監査済みstable Action、DND/Focusは引き続きguided/未実装として別に扱う。

## 3. 処理インターフェース

必須処理と契約:

| 処理 | 契約 |
| --- | --- |
| `detectCurrentState()` | `known(value) / unsupported / unknown(reason) / policyManaged / error` を返す。推測しない |
| `validate()` | build、probe、parameter、権限、policy、dependency、conflict、disk容量を確認。副作用なし |
| `createBackup()` | losslessな変更前状態と適用予定fingerprintを作る。永続化成功前にapply不可 |
| `apply()` | backupとpreconditionを受け、登録済みprimitiveだけを実行。idempotency key必須 |
| `verifyApplied()` | 再検出し、期待したobservable stateと照合。呼出し成功だけで成功にしない |
| `rollback()` | 元状態と現在状態を比較し、外部変更がなければ正確に復元 |
| `verifyRolledBack()` | 元の欠如状態・型・値を含めて照合 |
| `explainChanges()` | 結果、変更手段、対象resource、権限、restart、Update影響、復元範囲を構造化表示 |
| `troubleshooting()` | 登録済みerror codeごとの安全な案内。任意commandを提案・実行しない |

各処理は UI 文字列ではなく型付き request/response を使う。Action はUI表示、DB物理schema、ネットワークへ直接依存しない。

## 4. Action レジストリ

- Action ID → metadata、parameter schema、handler factory の対応はcompile-timeに固定する。
- stable、observation、experimental のregistryを分離する。
- 起動時に ID 重複、循環dependency、存在しないconflict、rollback decoder欠落を検査し、異常なら変更機能全体をsafe modeへ送る。
- elevated helper は独自の縮小allowlistを持ち、coreのregistryを信用して任意処理へdispatchしない。
- compatibility manifest は署名済みアプリ更新に同梱する。remote manifestは既存Actionを無効化できても、新しい変更先や値を注入できない設計とする。

### 変更手段の選択

同じ結果に複数手段がある場合、次の順を変えない。

1. Windows 公開 API
2. Microsoft 公式 CLI
3. WinGet
4. 公式の管理module
5. Microsoft が文書化した限定レジストリ設定
6. 検証済みの限定外部ツール連携

非公開COM、undocumented registry、画面自動操作しかない場合は stable Action にしない。設定画面へのguided Actionかexperimental候補に落とす。

`unverified_storage`はこの優先順位に入る変更手段ではない。固定位置を読み取ってsetter候補をレビューするための設計分類であり、文書化registryや限定externalへ読み替えてはならない。

## 5. 状態と事前条件

状態は boolean に丸めず、少なくとも次を区別する。

- `Known(desired value, evidence, observedAt)`
- `Unknown(reason)`
- `Unsupported(build/probe)`
- `PolicyManaged(authority if known)`
- `Conflict(current fingerprint)`
- `NeedsRestart`

preview で取得した状態には短い有効期限とfingerprintを持たせる。commit直前に再検出し、preview時と異なれば古い確認を使わず、変更内容を再表示する。

## 6. 複数 Action の transaction

```text
PLAN
  → PREFLIGHT_ALL
  → LOCK_RESOURCES
  → BACKUP_ALL
  → PREPARED (durable commit)
  → APPLY 1 → VERIFY 1 → journal commit
  → APPLY 2 → VERIFY 2 → journal commit
  → …
  → SUCCEEDED

途中失敗
  → 適用済みitemを逆順ROLLBACK
  → 各VERIFY_ROLLED_BACK
  → ROLLED_BACK / ROLLBACK_FAILED / RECOVERY_REQUIRED
```

### 詳細手順

1. 全Actionをcompatibility表へ照合する。
2. parameter schema、dependency、conflictを全件検証する。
3. dependencyのtopological orderを作り、同順位はAction IDで決定して再現可能にする。
4. `resourceKeys`を同じ決定順でlockし、deadlockを防ぐ。
5. 全Actionを再検出してprecondition fingerprintを作る。
6. 全backupを作り、SQLiteへ書き、`PREPARED`をdurable commitする。1件でも失敗すれば変更しない。
7. 順にapplyし、直後にverifyする。各段階の前後をjournalへ確定する。
8. 全件成功時だけtransactionを`SUCCEEDED`にする。
9. 失敗時は、そのtransactionが実際に変更したitemだけを逆順rollbackする。
10. rollback失敗を元のapply失敗で上書きせず、`RECOVERY_REQUIRED`として両方表示する。

Windows registry のtransaction APIに全体transactionを依存しない。Power API、process起動等をまたぐため、アプリレベルのsagaとdurable inverseで保証する。

## 7. resource、lock、lease

同じ Windows 状態を複数 Action が別名で書かないよう、実装はcanonical `resourceKey`を宣言する。

例:

- `registry:hkcu:64:<canonical-key>:<value>`
- `power:user-mode:ac`
- `power:user-mode:dc`
- `session:execution-state:system-required`
- `process:<file-identity>`

手動transactionは短期lock、ゲームプロファイルは所有者付きleaseを使う。

- 同じresourceへ同じdesired fingerprintを求める複数profileは1回だけ適用し、ownersを追加する。
- ownerが一つ終了しても、他ownerが残る間はrollbackしない。
- 最後のowner終了時に、最初のownerが保存したpre-stateへ戻す。
- desired fingerprintが異なる場合は後勝ちにせず、新しいprofileの該当Actionだけを`CONFLICT_BLOCKED`にする。
- profile途中失敗時は、そのprofileが新規取得したleaseだけを逆順解放する。

## 8. registry Action の固定モデル

registry path/valueはAction実装内の定数または限定enumから解決し、profile/UI/helper requestから自由形式で受け取らない。

backupには必ず次を保存する。

1. hive、canonical subkey、value name、32/64-bit view
2. key が存在したか
3. value が存在したか
4. value type
5. byte length と元のraw bytes
6. 適用予定type/value
7. 実際に適用したtype/value
8. Action/schema version、Windows build

lossless backup は `RegQueryValueExW` 相当のraw readを使う。文字列として正規化して保存しない。最小権限はread時とwrite時で分け、全権限を要求しない。

### setter根拠未承認の固定storage候補

`methodClass=unverified_storage`のActionは上記mutationモデルを実行しない。固定key/valueのread-only観測だけを許可し、raw DWORDをUI機能のオン／オフへ意味付けしない。`ActionMetadata.maximumTestedBuild`は現在`u32`のため、実機mutation試験なしを内部sentinel `0`で表し、IPCでは必ず`null`へ変換する。`0`を実在buildや互換範囲として扱ってはならない。旧versionが作成済みのdurable backupを持つ場合に限り、第三者変更を上書きしない既存rollback decoderを復旧用に保持する。

rollback時は現在値が実際の適用値fingerprintと一致するか先に確認する。一致しなければユーザー、GPO、他アプリによる外部変更なので自動上書きしない。元valueが無かった場合は、元keyが既存であることをbackupで確認したうえでvalueだけを削除する。Windows registryには「現在も空ならkeyを削除する」という原子的compare-deleteがないため、MVPの新規mutationは対象key自体が無い場合にbackup段階でfail-closedとし、keyを作らない。旧版backupの復旧でkeyが作成済みの場合もkey全体は削除せず、対象valueだけを安全に除去して空keyを残し、`RECOVERY_REQUIRED`として明示する。

Explorer/theme系setterの書込後は、まず`SHChangeNotify(SHCNE_ASSOCCHANGED, ...)`や対象別のbounded `WM_SETTINGCHANGE`で非破壊に反映を通知する。今回のstable ActionはExplorerを強制終了・再起動しない。通知で即時反映できなくてもregistry検証成功とUI反映を混同せず、「設定済み・再読込待ち」として結果を残す。

一次資料: [Registry functions](https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-functions)、[RegQueryValueExW](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryvalueexw)、[Registry access rights](https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-key-security-and-access-rights)。

## 9. API/CLI/process Action

### 公開 API

- backupはAPIが返すconfigured stateと、必要ならeffective stateを分ける。
- handle、COM object、allocated memoryの所有と解放をadapter内で閉じる。
- API成功と効果成功を分け、apply後に独立したget/detectで確認する。
- session API はlease owner thread/processを記録し、crash時にOSが自動解放するかを明記する。

### 公式 CLI

- CLI path、subcommand、optionをAction実装側で固定する。
- 引数は型付き要素として保持し、Windows adapterだけが検証済みのquoting規則で最終command lineへ変換する。ユーザー文字列をshell commandへ連結しない。
- exit code、bounded stdout、bounded stderr、timeoutを取得する。
- locale依存textを成功判定の唯一の根拠にしない。
- secret、usernameを含む出力を保存しない。

### process起動

- MVPはユーザーがfile pickerで選んだローカル固定drive上の実行ファイルを、handleから最終pathとfile identityを確認して登録する。
- relative path、環境変数展開、wildcard、UNC、reparse先未確認、任意shell verbを許可しない。
- local登録時にopaque app IDを発行し、Action/profileはそのIDだけを参照する。共有profileとAIはlocal path、app IDの新規作成、argvを指定できない。
- MVPは引数なしを基本とし、将来の可変値もappごとの固定schemaとallowlistに限定する。known shell/script host、system binary launcher、file associationを汎用appとして登録できない。
- `CreateProcessW`では検証済み絶対EXEを`lpApplicationName`へ明示し、必要な型付きargvだけをadapter内の試験済みWindows quotingで`lpCommandLine`へ変換し、handle継承を無効にする。argv配列を直接受けるAPIだとは仮定しない。
- 起動前から存在したprocessとPCカスタムが起動したprocessを区別する。
- rollbackで既定の強制終了をしない。明示同意がある場合も同一PID・creation time・image identityを再確認し、通常終了だけを要求する。

一次情報: [CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)、[Creating Processes](https://learn.microsoft.com/en-us/windows/win32/procthread/creating-processes)。

## 10. idempotency と再試行

- transaction/itemはUUIDのidempotency keyを持つ。
- 同じkeyのapplyが再送されたら、journalと現在stateを照合し、二重適用せず既存resultを返す。
- retry可能なのは明示された一時errorだけ。検証不一致、権限不足、unknown build、外部変更は自動retryしない。
- apply中に応答が失われた場合は、再apply前にdetectして `元状態 / 適用状態 / 第三の状態` に分類する。
- rollbackも同様にidempotentとし、既に元状態なら成功としてjournalだけを整合させる。

## 11. error とユーザー表示

結果は最低限次のstageを含む。

- `DETECT`, `VALIDATE`, `BACKUP`, `APPLY`, `VERIFY_APPLIED`
- `ROLLBACK`, `VERIFY_ROLLED_BACK`, `RECOVERY`

各errorは `code, stage, retryable, userMessageKey, diagnosticId` を持つ。Win32 error、exit code、HRESULTは内部診断に残すが、pathや個人情報を含むraw outputをそのままUI/ログへ出さない。

## 12. profile と AI からの利用

profile importで受理するのは以下だけである。

- profile metadata
- 登録済みAction ID
- Action schema内の安全な値
- built-in theme/reference ID
- 起動設定のうち共有を許可した抽象ID

任意code、script、binary、registry file、local path、任意URLはschemaに存在しない。AI出力も同じdata-only schemaへ通し、unknown fieldを拒否する。適用前に `explainChanges()` の結果を全件表示する。

## 13. 試験戦略

Actionごとに次のtable-driven試験を必須にする。

- 元value/stateが存在・不存在
- 異なる合法type/value、境界値、不正parameter
- policy managed、access denied、途中外部変更
- 未知build、API欠落、device切断
- backup直後、apply直前/直後、verify直前の強制終了
- rollback直前/直後、rollback verify失敗
- 同じrequestの二重送信
- 複数Actionのdependency/conflict/途中失敗
- 二つのgame profileによる同値共有・異値競合
- app update後に旧backup decoderで復元

自動化試験に加え、24H2/25H2と検証対象26H1のclean VM/実機でround-tripを行う。実機未試験のActionはIPCの`maximumTestedBuild`を`null`とし、自動適用不可にする。内部metadataが`u32`の間は`unverified_storage`かつ`guided`だけがsentinel `0`を使用でき、static contractで他分類への混入を拒否する。setter根拠未承認Actionの自動試験は、全候補についてvalidate/backup/apply拒否とraw保存値の不変を確認する。
