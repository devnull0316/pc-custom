# Rollback と変更タイムライン

## 1. 目的

rollback は「既定値を入れる」操作ではない。Totonoe が変更する直前の状態へ、値の欠如、型、複数resourceの組合せを含めて正確に戻す。Windows のシステム復元は補助にもできるが、通常の復元根拠にはしない。

## 2. 不変条件

1. lossless backup の durable commit 前に外部状態を変更しない。
2. backup とそれを読む decoder はversionを持つ。
3. apply後の実状態を検証し、実際の適用fingerprintを保存する。
4. rollback前に現在状態を再検出する。
5. 現在状態が適用fingerprintと異なる場合、外部変更を黙って上書きしない。
6. 複数itemは実際の適用順の逆順で戻す。
7. rollback失敗を元のapply失敗で隠さない。
8. app更新後も、保持期間内の旧backupを復元できるdecoderを残す。

## 3. backup envelope

すべてのbackupは共通envelopeとprimitive固有payloadを持つ。

### 共通情報

- backup ID、transaction ID、item ID、Action ID/version
- primitive kind、backup codec version
- created time、Windows base build/revision observation、app version
- owner (`manual`, `game-session:<id>`, `recovery`)
- resource key、precondition fingerprint
- desired state、実際のapplied fingerprint
- integrity hash。暗号学的署名ではなくDB破損検出用
- payload length とbounded schema
- backup scope (`user` / `privileged`) と、unknown buildを跨ぐ自動復元可否。後者は既定`false`

### 標準権限backupと特権backup

user-scope Actionのbackupは標準coreがOSから取得し、user-scope SQLiteへ保存する。将来のadmin Actionでは、user-scope DBやcoreから渡されたraw値を復元根拠にしない。elevated helper自身が適用直前の状態を再取得し、standard userが変更できないmachine-scope protected journalへ`PREPARED`をdurable commitしてから変更する。

coreが保持するのはopaque privileged backup ID、transaction/Action ID、非機密summary、helper resultだけである。rollback requestもopaque IDだけを送り、helperが保護journalのAction/version/resource、integrity、現在状態を再検証する。protected record欠落、改ざん、protocol不一致ではcoreの値を代用せず`RECOVERY_REQUIRED`にする。

protected storeにはraw backupと分離したpending indexを置き、要求元SIDにはread-only、Administrators/SYSTEMには更新権限を与える候補をTask 2で実証する。coreは起動時にuser DBのopaque IDとindexを照合し、どちらか一方だけに未完了recordがある場合も新規mutationを停止する。実際の復元はUAC後のhelperだけが行い、UAC取消時はpendingを残す。exact ACL/署名、別admin資格情報、user DB消失時の列挙は故障・攻撃試験に合格するまで未確定である。

coreはhelper起動前に`transaction ID + Action ID/version + local SID`をuser DBへprepared commitする。helperはwire上のSIDを信用せずIPC client tokenから要求元SIDを取得し、そのtupleをidempotency keyとしてprotected backupを作り、opaque IDを返す。helper commit後・coreのID保存前に停止した場合、次回coreはprepared itemとprotected pending indexを照合し、UAC後にhelperへverified requesting SIDの非終端record列挙を要求してtupleで再結合する。user DBが失われた場合もpending indexを無視せず、helper列挙が完了するまで新規mutationを止める。raw backupは列挙responseへ含めない。

### Registry payload

BRIEF の必須5項目を包含する。

| 項目 | 内容 |
| --- | --- |
| key存在 | hive/subkey/viewのkeyがあったか |
| value存在 | value nameがあったか |
| type | 元のregistry type |
| 元値 | byte lengthとraw bytes。文字列へ正規化しない |
| 適用値 | 予定値と実際値のtype/raw bytes |

加えて hive、canonical subkey、value name、32/64-bit view、Totonoeがkeyを作成したかを持つ。

復元:

- 元valueあり: 元type/raw bytesをそのまま設定する。
- 元valueなし: Totonoeの適用値と一致する場合だけvalueを削除する。
- 元keyなし: Totonoeが作り、現在空で、他の書込がないと証明できる場合だけkeyを削除する。それ以外は空keyを残して警告する。

### Power mode payload

- AC configured GUID、DC configured GUIDを別々に保存する。
- effective modeは診断用観測値であり、OEM/policy/battery saverで変わるため復元値にしない。
- rollbackは現在configured GUIDがTotonoe適用GUIDと一致する側だけを元GUIDへ戻す。片側競合は部分復元として明示する。

### Session/API payload

- sleep prevention: lease owner、flags、開始時刻、owner thread/process、API結果。
- process launch: 起動前に存在したinstance、Totonoeが起動したPID、creation time、file identity、終了方針。
- observation/guided Action: OS変更payloadを持たず、rollbackはno-opであることを明示する。

### Composite payload

theme等、複数primitiveを1 Actionで変える場合は子payloadを順序付きで保持する。子1だけ適用後に失敗した場合も、Action内で逆順復元できる。

## 4. transaction journal

| 状態 | 意味 | 次回起動時 |
| --- | --- | --- |
| `PLANNED` | previewのみ | 破棄可能 |
| `PREFLIGHTING` | 副作用なし検証中 | 再検証または破棄 |
| `PREPARED` | 全backupがdurable、未適用 | detectして未適用なら取消 |
| `APPLYING` | 1件以上の結果が不明かもしれない | itemごとにreconcile |
| `APPLIED` | item適用・検証済み | transaction継続または復元 |
| `SUCCEEDED` | 全item完了 | 通常履歴 |
| `ROLLING_BACK` | 逆順復元中 | rollbackをidempotentに再開 |
| `ROLLED_BACK` | 全item復元・検証済み | 通常履歴 |
| `ROLLBACK_FAILED` | 1件以上が復元失敗 | 詳細表示、再試行候補 |
| `RECOVERY_REQUIRED` | 判定不能/競合/decoder欠落 | 自動適用停止、ユーザー対応 |

itemは各stageの `startedAt`, `finishedAt`, `attempt`, result を持つ。`APPLYING`をcommitしてから外部変更し、結果をcommitしてから次itemへ進む。応答を失っても、次回は状態検出で判断できる。

## 5. reconcile アルゴリズム

非終端itemは現在状態を `original / applied / third / unknown` に分類する。

| 現在状態 | 処理 |
| --- | --- |
| original | 外部変更は残っていない。itemを未適用または復元済みとしてjournal整合 |
| applied | Totonoeの変更が残っている。自動rollback可能 |
| third | ユーザー、GPO、他アプリが変更。自動上書きせずconflict |
| unknown | API失敗、device不在、unsupported build。`RECOVERY_REQUIRED` |

「第三の状態」を強制的に元へ戻す操作は通常UIに置かない。将来提供する場合も、現在状態を新たにbackupし、具体的な上書き内容を再確認させる別transactionとする。

OS buildがbackup作成時のtested範囲外になった場合、raw状態が`applied`と一致するだけでは自動rollbackしない。Action/versionごとの`rollbackAcrossUnknownBuild`が明示承認され、同じ公開API/resource semanticsとdecoderをruntime probeで確認できた場合だけ自動復元できる。既定は`false`で、未承認なら`RECOVERY_REQUIRED`として、backup内容、公式Settings導線、CC確認済みの手動救済だけを提示する。

## 6. クラッシュ後復旧

起動順序:

1. DB schemaとintegrityを確認する。
2. `APPLYING`, `APPLIED`, `ROLLING_BACK`, active game sessionを列挙する。
3. protected pending indexとopaque IDも照合し、privileged未完了または不一致があれば新しいprofile検知、自動適用、app updateを停止する。
4. itemごとにreconcileする。
5. `applied`かつ現在buildでそのAction/versionのrollbackが承認済みのitemだけを逆順rollbackし、各々verifyする。
6. `third/unknown`は自動上書きせず、具体的resourceと理由を表示する。
7. 全件terminalになってから通常monitorを開始する。

Windows Application Recovery and Restart は早期再起動の補助として評価するが、強制終了、電源断、更新中断を網羅しない。唯一の復元根拠は事前commit済みjournalである。[Application Recovery and Restart](https://learn.microsoft.com/en-us/windows/win32/api/_recovery/)。

## 7. ゲームセッション復元

- profile開始時にsession IDと所有したleaseをdurable保存する。
- 同じresource/desired stateを複数profileが共有している間は、最初のpre-stateを保持する。
- profile終了時は、そのprofileが所有するleaseを逆順解放する。
- 最後のownerが外れた時だけpre-stateへrollbackする。
- 異なるdesired stateとの競合は適用時に止め、既存profileの状態を上書きしない。
- gameがcrashしてhandleがsignaledになった場合も通常終了と同じ復元経路を使う。
- Totonoeがcrashした場合は次回起動時にまず全未復元を戻し、gameがまだ動いていても古いsessionを暗黙再開しない。ユーザーが望む場合は復旧完了後に新しいpreviewから再適用する。

session API がprocess終了でOSにより自動解除される場合も、journalをreconcileし「OS側で解除済み」と記録する。永続Actionが残っている可能性とは分ける。

## 8. 変更タイムライン

1つの履歴行には、開始者、目的、Action一覧、変更前/後の結果表示、危険度、権限、検証結果、復元状態を示す。raw registry bytesや秘密は通常画面に出さない。

### この変更だけ戻す

- 対象Actionが後続Actionのdependency/preconditionになっていないか確認する。
- 同じresourceを後続transactionが変更していれば、単純rollbackせずdependency graphを示す。
- 独立していれば対象itemの現在状態を確認し、新しいrollback transactionとして記録する。

### この時点まで戻す

- 時点より後の「現在も有効な」変更を新しい順に選ぶ。
- resource単位で最新ownershipを解決し、既に戻されたitemを二重処理しない。
- 全件preflightと現在state確認をしてから逆順rollbackする。
- 途中失敗したら、復元済みitemを再適用して元の最新状態へ戻すことはしない。復元成功分と失敗分を明示し、安全側で停止する。

### 内容/結果を見る

- ユーザー結果、変更方法、対象resourceの抽象表示
- before/desired/observed-after
- build、Action version、試験範囲
- apply/verify/rollbackの各stageとdiagnostic ID

### 失敗項目だけ再試行

- retry前に全失敗itemを再検出する。
- `retryable`と登録された一時errorだけを既定で候補にする。
- access denied、未知build、外部競合、parameter不正、decoder欠落は自動retry不可。
- retryは新attemptとして記録し、過去の失敗を削除しない。

### ログ出力

- transaction/action/stage/error code、app/OS version、時間を構造化出力する。
- username、full local path、process command line、profile内の個人情報、IPC nonce、secret、raw stdout/stderrは除外またはredactする。
- ユーザーが内容をpreviewしてから保存する。

## 9. 部分復元と外部競合

複数resourceの一部だけが外部変更されていた場合:

1. 競合していないresourceは、Actionが部分復元を安全と宣言している場合だけ戻せる。
2. 原子的な組合せが必要なActionは全体を停止する。
3. UIは `2/3復元` のように表示し、全成功へ丸めない。
4. 残りは `ROLLBACK_FAILED` または `RECOVERY_REQUIRED` としてtimelineに残す。

例えばAC/DC power modeは独立したuser preferenceなので片側だけ復元可能だが、themeの2値が混在表示を生む場合はAction定義がall-or-stopを選べる。

## 10. アプリ更新と保持

- active/nonterminal transactionがある間はアプリ更新を開始しない。
- DB migration前に整合性検査とbackupを行う。
- update packageは、保持中backupの全codec versionに対応するrollback decoderを含む。
- Actionを廃止してもdecoderと説明metadataは履歴保持期間中残す。
- compatibility manifestの更新だけでbackup形式や復元先を変えない。
- OS buildを跨ぐrollback許可はAction/version単位で既定falseとし、signed compatibility evidenceだけで狭く許可する。
- retention policyで古い成功履歴を削除する場合も、現在有効な変更、active lease、未復元、失敗履歴は削除しない。
- backup削除前にその変更が既に元状態かを再検証する。

## 11. 故障注入試験

各Actionと複数Action transactionで、次の直前・直後にprocessを強制終了する。

- backup write / `PREPARED` commit
- item `APPLYING` commit / 外部変更 / result commit
- verify
- rollback開始 / 外部復元 / verify / result commit
- DB migration、app updaterによる終了

加えて、電源断相当、disk full、DB locked/corrupt、API timeout、device抜去、GPOによる書戻し、他アプリの同時変更、PID再利用、Windows build変更を試す。

合格条件:

- backup未確定の変更が1件もない。
- 次回起動で非終端itemを必ず列挙できる。
- original/applied/third/unknownを誤って成功へ丸めない。
- registry値の不存在、型、raw bytesがround-trip一致する。
- rollback失敗が明示され、後続自動適用が停止する。

## 12. 手動救済

自動復元できない場合も、Totonoeは任意scriptを生成・実行しない。画面には次を示す。

- 何を変更したか
- 保存済みの元状態を人が読める範囲で説明
- 自動復元できない理由
- Microsoft公式設定画面または一次資料へのリンク
- 個人情報を除いた診断export

「修復」ボタンで未検証commandを実行することはしない。
