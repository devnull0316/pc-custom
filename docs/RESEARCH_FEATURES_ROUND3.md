# Feature Research Round 3

調査日: 2026-07-29

## 調査の前提

Round 1 / Round 2 と `docs/STATUS.md` を基準に、既に採用・保留・却下した単機能は候補から外した。今回の狙いは、PC Custom の核である「使う前の状態を覚える」「変更を目で確認できる」「自分が変えた分だけ戻す」を、まだ触れていない領域と複数操作のまとまりへ広げることである。

候補は次の境界を守る。

- Defender、ファイアウォール、Windows Update、ページファイル、BCD、サービス群、プロセス注入、ゲームファイルには触れない。
- 管理者権限を要求しない。任意コマンド、任意スクリプト、利用者が自由入力する起動引数も扱わない。
- 「速くなる」「FPS が上がる」など、外部効果を測れない性能訴求はしない。
- getter が setter の値を返しただけでは採用しない。Windows の別経路、別プロセス、または目に見える標準 UI で効果を確認する。
- 完全に戻せない案は「戻せる」と表示しない。読み取り専用には、変更履歴も「戻す」ボタンも作らない。

ラベルの意味:

- **A — 未着手領域**: Round 1 / Round 2 で変更機能を出していない Windows 領域。
- **B — まとめて戻せる**: 複数の既存操作を一つの利用場面として開始し、まとめて終了できる。
- **C — 新規 API なし**: 現在実装済みの action / probe / rollback 部品だけで最小版を作れる。

## 結論一覧

| 順位 | 利用者に見せる名前 | ラベル | 初期判断 | 戻し方 |
|---:|---|---|---|---|
| 1 | 一時ワークスペース — 始める前の机へ戻す | B / C | 小さく実装候補 | 動かした窓と変更した設定を一括復元 |
| 2 | 容量がいつ減ったか分かる履歴 | A | 読み取り専用で実装候補 | 変更しないため対象外 |
| 3 | 画面共有前後の準備・復帰 | B / C | 小さく実装候補 | 自動変更だけ一括復元、手動項目は案内 |
| 4 | 場所ごとの既定プリンター | A | proof 後に限定実装 | 直前の既定プリンターへ復元 |
| 5 | 12時間 / 24時間表示を一時切替 | A | proof 後の小候補 | 二つの書式文字列を復元 |
| 6 | 仕事用 VPN を一時接続 | A | research / manual-only | 同一実行中のみ切断可、障害復旧は案内 |
| 7 | コントラストテーマを短時間試す | A | research / 明示 opt-in | 短時間試用後に即時復元 |

---

## 1. 一時ワークスペース — 始める前の机へ戻す

**ラベル: B / C**

### 1) 利用者が得る結果

「作業を始める」を押すと、必要なアプリと窓がいつもの位置にそろう。「終わる」を押すと、開始前に開いていた窓の位置、表示状態、PC Custom が変えた見た目や作業中設定だけが元へ戻る。

最小版は、既に許可されたアプリの起動、既存窓の配置、外観シーン、モードリボン、必要ならスリープ抑止を一つのセッションに束ねる。アプリの強制終了、未保存文書の判定、任意引数による起動はしない。

### 2) なぜ必要か — 実在する声

- [Windows 11 にワークスペース保存と復元がほしい](https://www.reddit.com/r/Windows11/comments/1j28rik/) — 2025-03-03、+6。アプリと窓をまとめて保存し、後から同じ作業状態を開きたいという直接要望。票は少なく、限定的な声として扱う。
- [既存の窓を素早く切り替える用途では Workspaces が遅い](https://www.reddit.com/r/PowerToys/comments/1re6wcq/) — 2026-02-25、+1。毎回アプリの起動確認が入り、既に開いている窓の文脈切替には重いという声。単独投稿であり需要量の根拠にはしない。
- [PowerToys Workspaces 公開時の反応](https://www.reddit.com/r/Windows11/comments/1f8w0nt/) — 2024-09-04、+80。便利という反応と同時に、仮想デスクトップ対応や、既存アプリと新規起動の挙動への質問が出ている。
- [PowerToys Workspaces の複数仮想デスクトップ対応要望](https://github.com/microsoft/PowerToys/issues/35407) — 2024-10-12、リアクション数は調査時に確認できず。この issue は duplicate 扱いであり、需要規模の証拠ではなく境界事例として使う。

声が示すのは「並べる」だけでなく「今ある作業から別の作業へ移り、あとで元へ戻る」需要である。ただし、票数は PowerToys 公開反応を除き小さい。大衆向けの確定需要ではなく、PC Custom の既存利用者に対する統合価値として評価する。

### 3) 競合と埋まっていない隙間

[PowerToys Workspaces](https://learn.microsoft.com/en-us/windows/powertoys/workspaces) は、複数アプリを所定位置に起動し、既存窓を移動できる。起動状況も表示する。一方、公式説明の中心は workspace の起動・配置であり、「この一時作業を終えて、開始前の机へまとめて戻る」ことではない。

PC Custom の隙間は次の三点に絞る。

- 新しい完成形を保存するのではなく、開始時に毎回「戻る場所」を採取する。
- 窓配置だけでなく、既に安全性を確認済みの外観・スリープ抑止などを同じ一時セッションに含める。
- 各 action の durable marker と外部競合判定を保ち、他アプリや利用者が途中で変えた値を上書きしない。

仮想デスクトップの再現、アプリ内部の文書・タブ・未保存状態の復元は隙間に含めない。そこまで約束すると正確な復元ではなくなる。

### 4) 使える公式 Windows API / 文書

新規 API は不要で、現在の次の実装済み部品を束ねる。

- `setup.window_layout`: [GetWindowPlacement](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowplacement)、[SetWindowPlacement](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowplacement)、[WINDOWPLACEMENT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-windowplacement)
- `setup.launch_apps`: 現在の allowlist と既存の起動処理だけを使用
- `appearance scene`、`mode ribbon`、`session.prevent_sleep`: STATUS に記録済みの既存 action / UI

新しい OS 変更 API を足す案ではなく、既存 action を一つの親セッションへ関連付ける製品機能である。

### 5) 元に戻せるか

**条件付きでまとめて戻せる。**

開始時に、対象窓の同一性、位置、通常・最小化・最大化状態と、採用した各 action の元値を保存する。終了時は action ごとに「現在値が PC Custom の適用値のままか」を確かめ、自分が動かした対象だけを逆順で復元する。

次は戻せないため、最初から変更対象にしない。

- 起動したアプリを自動終了すること。未保存文書やバックグラウンド処理を安全に判定できない。
- アプリ内部のタブ、文書、ゲーム、ログイン状態。
- セッション中に終了・再生成されて別物になった窓。
- 利用者または他アプリがセッション中に再配置した窓。外部競合として残し、理由を表示する。

「終了」は「PC Custom が動かした窓と設定を戻す」であり、「PC 全体を過去の完全な状態へ巻き戻す」ではない。

### 6) 難易度と最大のリスク

**難易度: 中。** 新しい Windows API より、複数 action の親子履歴、部分失敗、逆順 rollback、再起動後の recovery 表示を一貫させる作業が中心になる。

最大のリスクは、同じタイトルや同じ実行ファイルの別窓を誤って戻すこと。既存の exact window identity を崩さず、開始時に捕捉できた窓だけを扱う必要がある。もう一つのリスクは「作業を終える」がアプリ終了まで含むと誤解されることなので、UI では「窓と設定を戻す」と明記する。

### 7) PC Custom の核との適合

非常に高い。単発の tweak を増やすのではなく、既存の可逆 action を「一時的な利用場面」に昇格させる。開始前 snapshot、適用後 verification、durable history、外部競合を避ける rollback という既存設計が、そのまま差別化になる。

また、71 action を増やさずに利用価値を増やせる。既存の 16 mutable action を無秩序に束ねるのではなく、互いに独立して戻せる少数の action だけを利用者が明示選択する。

### 8) 外から効いたとどう測るか

setter の readback だけでは合格にしない。

- テスト用の別プロセスで複数の標準窓を作り、適用前・適用後・終了後の画面座標と表示状態を、その別プロセス側から記録する。
- 適用後に各窓が期待するモニターの作業領域内に見え、終了後に元の座標・表示状態へ戻ったことを確認する。
- セッション中に一つの窓を人為的に別位置へ動かし、終了時にその窓だけを上書きせず「外部変更」として残すことを確認する。
- 外観など同梱した action は、それぞれが既に持つ独立した outward verification を通す。親セッションの成功は全子 action の成功を意味せず、成功・失敗・競合を個別表示する。

アプリ内部の文書状態や「集中できたか」は測れないため、効果指標に含めない。

---

## 2. 容量がいつ減ったか分かる履歴

**ラベル: A**

### 1) 利用者が得る結果

「空き容量が昨日より 8 GB 減った」「同じ期間にダウンロードが 5 GB、動画が 2 GB 増えた」のように、少数の利用者フォルダーとドライブ全体の変化を時系列で見られる。ファイルを削除せず、原因を断定せず、調べ始める場所だけを示す。

保存するのは時刻ごとの集計値で、ファイル名や個別パスの履歴ではない。既存の一回限りの空き容量チェックを、上限付きの履歴へ発展させる。

### 2) なぜ必要か — 実在する声

- [C ドライブが埋まり続け、Windows の「システム」分類では場所が分からなかった](https://www.reddit.com/r/WindowsHelp/comments/1tolfhj/) — 2026-05-26、+30。最終的に約 200 GB の WAL ファイルを発見したという事例。
- [7 GB 空けても一週間でまた消えた](https://www.reddit.com/r/WindowsHelp/comments/1u2k2fw/) — 2026-06-11、+3。本人は何も追加していないつもりで、時間差の比較を必要としている。票は少なく限定的。
- [ドライブが埋まるが、解析ツールの結果も理解しづらい](https://www.reddit.com/r/WindowsHelp/comments/1tx82gs/) — 2026-06-05、+8。
- [毎日約 5 GB を消しても、起動後にまた埋まる](https://www.reddit.com/r/WindowsHelp/comments/1qwlifi/) — 2026-02-05、+18。
- [空き容量が画面上でゼロへ減っていく](https://www.reddit.com/r/WindowsHelp/comments/1qrw1sj/) — 2026-01-31、+16。

複数の独立投稿が「今どこが大きいか」だけでは足りず、「いつ増えたか」「前回から何が変わったか」を求めている。個々の投稿票は大きくないが、同じ痛点が反復している。

### 3) 競合と埋まっていない隙間

[WizTree](https://diskanalyzer.com/) は現在の大きなファイル・フォルダーを高速に可視化し、[TreeSize](https://www.jam-software.com/treesize) は詳細な可視化、レポート、上位版で期間比較も提供する。Windows のストレージ画面も現在のカテゴリ別容量を示す。

PC Custom が埋める隙間は、万能ファイル解析ではない。

- 管理者権限なしで、利用者フォルダーの少数カテゴリとドライブ空き容量だけを定期的に比べる。
- PC Custom の action 履歴と同じ時間軸に置き、「同じ時間帯に変化した」ことまでを示す。
- 個別ファイル名を収集せず、削除機能へ直結させない。
- 巨大な treemap を読めない人にも、「前回から増えた場所」だけを短く示す。

PC Custom の操作と容量減少に因果関係があるとは表示しない。同じ期間に起きた相関だけを提示する。

### 4) 使える公式 Windows API / 文書

- ドライブ全体の空き容量: [GetDiskFreeSpaceExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw)
- 利用者フォルダーの安全な起点: [Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/known-folders)、[SHGetKnownFolderPath](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath)
- 上限付き列挙: [FindFirstFileExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw)

既存の `storage.free_space_check` と、STATUS にある時間・件数・深さの固定 budget、reparse point を追わない走査規則を再利用する。USN Journal、MFT の直接読取、VSS、管理者専用領域には進まない。

### 5) 元に戻せるか

**戻す対象はない。読み取り専用である。**

PC のファイルや Windows 設定を変えない。利用者が明示的に「履歴を消す」を選んだ場合だけ、PC Custom が保持した集計履歴を削除する。これは OS 状態の rollback ではないため、通常の action history や「元に戻す」には載せない。

ファイル削除、クリーンアップ、一時ファイル除去、フォルダー移動はこの機能に含めない。

### 6) 難易度と最大のリスク

**難易度: 中。** API 自体は単純だが、走査の時間上限、アクセス拒否、hard link、sparse file、圧縮、クラウド placeholder、同時更新によって「フォルダー合計」と「ドライブ空き容量」が一致しないことを前提に設計する必要がある。

最大のリスクは、バックグラウンド走査がディスク負荷とプライバシー不安を生むこと。初期版は Documents、Downloads、Desktop、Pictures、Videos など利用者が選んだ既知フォルダーに限定し、名前やパスを保存せず、AC 電源時などの暗黙条件も勝手に追加せず、利用者が指定した頻度だけで走査する。Windows、Program Files、他ユーザー、ブラウザー内部、メールデータは走査しない。

### 7) PC Custom の核との適合

高い。PC Custom が「何を変えたか」に加えて、「PC の状態がいつ変わったか」を安全に見せられる。変更を増やさず、観測と説明責任を強化する方向であり、39 display-only action の蓄積とも整合する。

一方、ファイル管理・削除ツールへ広げると核から外れる。履歴、差分、次に調べる場所の提示までに留める。

### 8) 外から効いたとどう測るか

読み取り機能なので、「効いた」は表示値の正しさと負荷上限で測る。

- テスト専用フォルダーに既知サイズのファイルを作り、スナップショット間のカテゴリ増分が実際の allocation size の許容範囲内で一致することを確認する。
- 同時に別プロセスからドライブ空き容量を取得し、増加・削除後の方向と概算量が一致することを確認する。
- テストファイルを削除し、次の採取でカテゴリ値と空き容量が反対方向へ戻ることを確認する。
- reparse point を含む循環構造、アクセス拒否、大量ファイルを試し、追跡しないこと、固定 budget で打ち切ること、打ち切りを「完全な集計」と表示しないことを確認する。
- 収集 DB を検査し、ファイル名・個別パス・内容が保存されていないことを確認する。

フォルダー合計とドライブ差分が完全一致するとは保証できない。更新中ファイル、予約領域、hard link、クラウド状態などがあるため、製品表示にも「概算」「未走査領域あり」を出す。

---

## 3. 画面共有前後の準備・復帰

**ラベル: B / C**

### 1) 利用者が得る結果

会議前に一つの画面で、通知、マイク、既定の音声機器、見せたくない窓、スリープの不安を確認できる。PC Custom が安全に自動変更できる項目だけを「共有の準備」として適用し、会議後にまとめて元へ戻す。通知のように現在の証拠では安全な公的 setter を採用できない項目は、Windows の該当設定を開いて利用者自身に確認してもらう。

「すべて非表示にした」「通知は絶対出ない」とは約束しない。これは漏えい防止の保証機能ではなく、忘れ物を減らす準備セッションである。

### 2) なぜ必要か — 実在する声

- [Teams の全画面共有中に機密チャット通知が表示される](https://www.reddit.com/r/TeamsAdmins/comments/1s1zr5i/any_way_to_globally_disable_notifications_while/) — 2026-03-24、+3。複数の pilot 利用者で起き、個人ごとの手動設定では運用負担が高いという管理者の声。票は少なく限定的。
- [Teams 共有中にメッセージが参加者全員へ見えた](https://www.reddit.com/r/MicrosoftTeams/comments/17rn1e9/) — 2023-11-09、+0。Do not disturb が常に自動で効くわけではないという体験。支持票は確認できない。
- [Teams でセンシティブな通知が共有画面に出る](https://www.reddit.com/r/MicrosoftTeams/comments/1047qtg/) — 2023-01-05、+1。低票の限定事例。
- [授業の Zoom 共有中に Steam 通知が出た](https://www.reddit.com/r/Zoom/comments/1uyo48o/) — 2026-07-17、+1。直近の直接事例だが、票は少なく限定的。

センシティブな問題である一方、取得できた票数はすべて小さく、需要規模は断定できない。ただし、一度の失敗コストが高く、既存部品の再構成で試せるため候補に残す。

### 3) 競合と埋まっていない隙間

Windows には [Notifications and do not disturb](https://support.microsoft.com/en-us/windows/experience/notifications-and-do-not-disturb-in-windows) と [Focus](https://support.microsoft.com/en-us/windows/experience/focus-stay-on-task-without-distractions-in-windows) がある。Do not disturb は手動または規則で有効化できるが、priority app の通知は許可できる。PowerToys の [Awake](https://learn.microsoft.com/en-us/windows/powertoys/awake) は電源プランを変えずに一時的に PC を起こしておける。

隙間は個々の設定ではなく、会議直前の「確認 → 安全にできる分だけ適用 → 会議後に復帰」の一本化である。

- Windows、Teams、Zoom の通知抑止を横断的に保証する競合ではない。
- PC Custom は、既存 readiness のマイク・音声機器・通知状態と、窓配置、スリープ抑止、モードリボンを同じチェックリストに置ける。
- 自動化できない通知は、状態を偽らず Windows の正規設定画面へ案内する。

### 4) 使える公式 Windows API / 文書

最小版は新規 API なしで構成する。

- 既存の readiness: 通知状態、マイク状態、既定音声機器の表示
- 既存の `session.prevent_sleep`
- 既存の `setup.window_layout` と mode ribbon
- 設定を開く必要がある場合: [Launch the Windows Settings app](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings) に載る固定 `ms-settings` URI のみ

通知の自動抑止 setter は今回の採用根拠にしない。公式サポート文書に操作方法があっても、PC Custom が安全に所有・復元できる public desktop API の証拠とは別だからである。

### 5) 元に戻せるか

**自動変更した項目だけ、まとめて戻せる。**

スリープ抑止、窓配置、モードリボンなど PC Custom が既存 action として適用したものは、通常の durable marker と外部競合判定を保ったまま逆順に戻す。

次は自動 rollback の対象外である。

- 利用者が Windows 設定で手動変更した通知・Do not disturb。
- Teams、Zoom、ブラウザーなど各アプリ内の通知や共有設定。
- 会議中に利用者が手動で変えたマイク・音声機器。
- 画面共有そのものの開始・終了。

手動確認項目は「確認済み」のチェックを同じセッションに記録できるが、変更値を PC Custom が所有したとはみなさない。終了時には「手動で変えた可能性がある項目」を再確認する案内を出す。

### 6) 難易度と最大のリスク

**難易度: 低〜中。** 最小版は既存部品の構成と文言が中心。難所は技術ではなく、プライバシー保証に見える誤解を避けること。

最大のリスクは、利用者が「準備完了」を「通知や個人情報が絶対に映らない」と受け取ること。Windows の priority notification、アプリ独自通知、ブラウザー通知、共有範囲、別モニターなど、PC Custom が制御・観測できない経路がある。表示は「確認した項目」と「確認できない項目」を分け、緑一色の安全保証にしない。

### 7) PC Custom の核との適合

高い。既存の display-only probe と reversible action を、失敗しやすい実生活の一場面へまとめ直す。新しい危険な setter を増やさず、変更前の確認と終了時の復帰を製品の中心にできる。

また、単発の「マイクをミュートする」は Round 2 で実装済みなので再提案しない。この案の価値は、会議準備の複数要素を一つの一時セッションとして扱う点にある。

### 8) 外から効いたとどう測るか

項目別に判定し、総合的な「漏えい防止率」は作らない。

- スリープ抑止は既存の outward verification を使い、適用中の Windows power request 状態を別の観測経路で確認し、終了後に消えることを確かめる。
- 窓配置は別プロセス側から座標と表示状態を観測し、共有用レイアウトと終了後の元配置を確認する。
- マイクと既定音声機器は readiness の独立 probe で表示し、実際の会議アプリでミュートされたか、正しい音が届くかまでは保証しない。
- 通知は Windows 設定への案内と利用者の確認に留める。priority app やアプリ独自通知まで抑止できたかを一般的に観測する手段はないため、「測定不能」と明記する。
- E2E ではテスト用通知アプリとテスト会議画面を使い、既知の通知が見えるケースを再現する。ただし、これに成功しても全アプリの通知非表示保証にはしない。

---

## 4. 場所ごとの既定プリンター

**ラベル: A**

### 1) 利用者が得る結果

「自宅」「職場」「ラベル印刷」など利用者が付けた場面を選ぶと、既にインストール済みのプリンター一台を今回の既定にする。作業が終われば直前の既定プリンターへ戻る。プリンターやドライバーの追加、印刷設定の一括変更、実際の印刷はしない。

場所を GPS やネットワークから自動推測せず、利用者が明示的に選ぶ。初期版は Windows が自動管理していない環境だけを対象にする。

### 2) なぜ必要か — 実在する声

- [自宅と職場で既定プリンターを場所に応じて変えたい](https://www.reddit.com/r/printers/comments/1jlx7au/) — 2025-03-28、+1。batch や scheduled task の回避策は不安定という直接要望。単独かつ低票で、需要は限定的。
- [Webex が既定プリンターを変えてしまう](https://www.reddit.com/r/windows/comments/gmckpi/) — 2020-05-18、+13。Windows の自動管理を切っても別アプリが変更したという外部競合の実例。
- [四つのアプリと四台のプリンターで毎回選択が必要](https://www.reddit.com/r/printers/comments/1spy44u/windows_11_is_ruining_my_small_print_shop_hobby/) — 2026-04-19、+7。複数プリンター運用の反復負担を示す。

需要は広いとは言えないが、変更対象と復元値が明瞭で、PC Custom の exact rollback を説明しやすい。

### 3) 競合と埋まっていない隙間

[Windows の既定プリンター設定](https://support.microsoft.com/en-us/windows/hardware/printer/set-a-default-printer-in-windows) は、特定プリンターを手動で既定にするか、最後に使ったプリンターを Windows に管理させる。後者は mobile Windows device の場所が変わると、その場所で最後に使ったプリンターへ変わる場合がある。

PC Custom の隙間は「最後に使ったもの」ではなく、明示的な作業セッションと exact rollback である。

- 利用者が「今日はラベル印刷」と選び、終わったら開始前の既定へ戻す。
- Windows の自動管理設定自体は変えない。
- Webex など別アプリが途中で既定を変えたら、それを上書きせず外部競合として止める。

企業のプリント管理、ドライバー配布、プリンター追加、印刷ジョブ制御は対象外。

### 4) 使える公式 Windows API / 文書

- 現在の既定取得: [GetDefaultPrinter](https://learn.microsoft.com/en-us/windows/win32/printdocs/getdefaultprinter)
- 既に存在するプリンターの列挙: [EnumPrinters](https://learn.microsoft.com/en-us/windows/win32/printdocs/enumprinters)
- 現ユーザーの既定変更: [SetDefaultPrinter](https://learn.microsoft.com/en-us/windows/win32/printdocs/setdefaultprinter)
- 別経路で既定を観測: [PRINTDLGEXW](https://learn.microsoft.com/en-us/windows/win32/api/commdlg/ns-commdlg-printdlgexw) の `PD_RETURNDEFAULT`
- 印刷ダイアログの公式概説: [Displaying a Print Property Sheet](https://learn.microsoft.com/en-us/windows/win32/printdocs/printer-output)

`SetDefaultPrinter` は同期的で、ネットワーク・サーバー・ドライバー構成によって停止時間が伸び得るため、UI thread で直接待たない設計が必要になる。

### 5) 元に戻せるか

**条件付きで戻せる。**

適用前の既定プリンター名を保存し、候補プリンターが `EnumPrinters` に存在することを確認して適用する。終了時は次の全条件を満たす場合だけ元へ戻す。

- 元のプリンターが今も列挙できる。
- 現在の既定が PC Custom の適用したプリンターのまま。
- Windows の「既定プリンターを管理する」モードを PC Custom が変更していない。

現在値が別物なら、利用者または他アプリによる変更として rollback を止める。元のプリンターが削除・オフライン・名前変更された場合も、自動で代替を選ばず recovery-needed とする。

### 6) 難易度と最大のリスク

**難易度: 中。** 単一値の変更に見えるが、プリンター名、ドライバー、ネットワーク待ち、Windows 自動管理との関係を扱う必要がある。

最大のリスクは、別アプリや Windows が既定を変える競合と、spooler / ネットワーク待ちで UI が固まったように見えること。変更前後の timeout、非同期 UI、候補の再列挙が必要。既定を変えても、アプリが独自に前回プリンターを記憶する場合があるため、「全アプリの印刷先が変わる」とは表示しない。

### 7) PC Custom の核との適合

中〜高。利用者単位の一時変更で、元値・適用値・競合を明確にできる。仕事の場面を切り替え、後から自分の変更だけ戻すという核に合う。

ただし利用者層は限定的で、プリンター未使用 PC では完全に不要。機器が二台以上列挙できる場合だけ候補を見せ、トップレベルの常設機能にはしない。

### 8) 外から効いたとどう測るか

`GetDefaultPrinter` の readback だけでは不十分。

- 別のテストプロセスで `PRINTDLGEXW` を `PD_RETURNDEFAULT` 付きで呼び、UI を出さずに返る `DEVNAMES` が適用先を指すことを確認する。
- rollback 後に同じ別プロセスを新しく起動し、開始前のプリンターへ戻ったことを確認する。
- 実印刷はしない。紙、インク、企業プリントキューへ副作用を出さない。
- セッション中に別プロセスで既定を第三のプリンターへ変え、PC Custom が終了時に上書きせず外部競合を報告することを確認する。

個々のアプリがその既定を採用したかはアプリ固有で、一般には測れない。その限界を UI に明記する。

---

## 5. 12時間 / 24時間表示を一時切替

**ラベル: A**

### 1) 利用者が得る結果

会議、配信、海外との作業など必要な間だけ、現在のユーザーの短い時刻表示を 12 時間または 24 時間へ切り替え、終了時に元の表記へ戻す。日付、地域、タイムゾーン、システムアカウント、ロック画面まで変わるとは約束しない。

利用者向け表示は「13:05 で表示」「1:05 PM で表示」のプレビューを中心にし、内部の書式名を前面に出さない。

### 2) なぜ必要か — 実在する声

- [Windows 11 の時刻を 24 時間表示にしたい](https://www.reddit.com/r/Windows11/comments/oelew3/) — 2021-07-06、+38。地域設定と表示形式の分かりにくさをめぐる声。
- [24 時間設定なのにロック画面だけ 12 時間になる](https://www.reddit.com/r/Windows11/comments/ufae3c/) — 2022-04-30、+80。好みが分かれることと、Windows 内でも適用範囲が一様でないことを示す。今回、ロック画面を対象外とする根拠にもなる。
- [更新後に 24 時間表示が消えた／合わなくなった](https://www.reddit.com/r/windowsinsiders/comments/x26yyg/) — 2022-08-31、+5。票は少なく限定的。
- [12 時間表示の選択肢が見つからず、レジストリ変更で不自然な表示になった](https://www.reddit.com/r/windows11help/comments/1jlry7a/) — 2025-03-28、+1。単独の低票だが、設定経路の分かりづらさと手作業の失敗を示す。

大きな未充足市場の証拠ではない。常時の好みは Windows 設定で十分であり、この候補は「短時間だけ切り替えて確実に戻す」という小さな場面に限る。

### 3) 競合と埋まっていない隙間

Windows の言語と地域の表示形式から常時変更できる。多くの利用者にはそれで足りる。

PC Custom の隙間は、設定場所の置き換えではなく一時セッションである。

- 画面共有や海外相手の作業中だけ、誤読しにくい表示へ変える。
- 元のカスタム書式を文字列として保存し、終了時にそのまま戻す。
- 変更前に 13:05 の見え方を示し、地域やタイムゾーン変更とは別物だと明示する。

タスクバーだけを独立変更する非公開手段、レジストリ直書き、ロック画面や全ユーザーへの強制は採らない。

### 4) 使える公式 Windows API / 文書

- 現ユーザー書式の取得: [GetLocaleInfoEx](https://learn.microsoft.com/en-us/windows/win32/api/winnls/nf-winnls-getlocaleinfoex)
- 短い時刻の書式: [LOCALE_SSHORTTIME](https://learn.microsoft.com/en-us/windows/win32/intl/locale-sshorttime)
- 現ユーザー override の設定: [SetLocaleInfoW](https://learn.microsoft.com/en-us/windows/win32/api/winnls/nf-winnls-setlocaleinfow)
- 設定変更の通知: [WM_SETTINGCHANGE](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange)。locale 変更時の `lParam` は `intl`
- 固定時刻の別経路プレビュー: [GetTimeFormatEx](https://learn.microsoft.com/en-us/windows/win32/api/datetimeapi/nf-datetimeapi-gettimeformatex)

設定対象は `LOCALE_SSHORTTIME` と `LOCALE_STIMEFORMAT` の二つに限定する。`SetLocaleInfoEx` は存在しないため、文書化された `SetLocaleInfoW` を使う。

### 5) 元に戻せるか

**現ユーザーの対象二値に限り、条件付きで戻せる。**

適用前に二つの書式文字列をそのまま保存する。適用後は OS が正規化した実値を再取得し、それを applied value として履歴に残す。終了時は現在の二値が applied value のままなら、保存した元文字列を両方戻して `WM_SETTINGCHANGE` を通知する。

どちらか一方でも利用者または別アプリに変更されていれば、二値を一組の外部競合として自動復元しない。ロック画面、システムアカウント、他ユーザー、アプリ独自書式は snapshot も変更もできないため、戻せる範囲に含めない。

### 6) 難易度と最大のリスク

**難易度: 低〜中。** API 呼び出しは小さいが、locale ごとの区切り、AM/PM 文字列、アプリの設定反映タイミング、二値の整合を proof する必要がある。

最大のリスクは、時計だけを変えるつもりで他アプリの時刻表示まで変わること。Microsoft 文書も current locale の user override が全アプリへ影響し得ると注意している。必ずプレビュー、明示確認、一時タイマーを付け、プロファイルへの自動組み込みは初期版で許可しない。

### 7) PC Custom の核との適合

中。変更前 snapshot と exact rollback はきれいに適用できるが、日常的な価値は狭い。Windows 設定の単なるショートカットにならないよう、「一時切替」「自動的な終了案内」「元のカスタム書式の保全」が成立する場合だけ価値がある。

Round 1 / Round 2 の外観機能とは別で、地域・時刻表示の未着手領域を扱う。ただし上位候補より優先度は低い。

### 8) 外から効いたとどう測るか

- setter と別の新規テストプロセスで、`GetTimeFormatEx` に固定時刻 13:05 を渡し、期待する 24 時間表記または time marker 付き 12 時間表記になることを確認する。
- Windows シェルのタスクバー時計を UI Automation で読み、24 時間なら 13、12 時間なら 1 と AM/PM 相当が見えることを確認する。ただし locale によって marker の文字が異なるため固定英字にはしない。
- rollback 後に新しいテストプロセスで同じ固定時刻を整形し、元の表記と一致することを確認する。
- タスクバーが更新しない環境では setter readback だけで成功にせず、「設定値は変更済みだがシェル表示を確認できない」と分ける。

ロック画面はセッションを切り替えず安全に自動観測できず、対象外である。アプリ独自の表示も一般的には測れない。

---

## 6. 仕事用 VPN を一時接続

**ラベル: A**

### 1) 利用者が得る結果

Windows に既に登録済みの仕事用 VPN を、必要な作業の間だけ接続する。接続前に未接続だったことを確認でき、同じ PC Custom 実行中に自分が開始した接続だけを終了できる。

VPN の新規作成・編集、資格情報の保存、パスワード入力の代行、常時接続、回線の自動選択はしない。ベンダー独自アプリや Azure VPN Client のプロファイルも対象外。

### 2) なぜ必要か — 実在する声

- [登録済み VPN の接続に毎回三段階の UI 操作が必要](https://www.reddit.com/r/Windows11/comments/13tv56x/) — 2023-05-28、+18。自動接続を求める直接要望。同じスレッドには Azure VPN app の接続は `rasdial` では扱えなかったという境界事例もある。
- [Windows 組み込み VPN に自動接続の選択肢がない](https://www.reddit.com/r/WindowsHelp/comments/1jamw96/) — 2025-03-13、+0。古い回答や第三者 VPN 向け回答が適用できないという声。票はなく、需要の強さではなく現行 UI の摩擦の例として扱う。

証拠は二件で強くない。さらにネットワーク接続は失敗時の影響が大きいため、実装優先ではなく research / manual-only 候補とする。

### 3) 競合と埋まっていない隙間

Windows のクイック設定や「ネットワークとインターネット」から接続でき、組織 VPN クライアントには自動接続や trusted network detection がある。汎用ランチャーや `rasdial` を使う回避策もある。

PC Custom の狭い隙間は次だけである。

- Windows 標準 RAS phone-book に既に存在する一件を、利用者が明示して一時接続する。
- 接続前の状態と、PC Custom が得た connection handle を記録する。
- 作業セッション終了時に、同一実行中に所有を証明できる接続だけを切る。
- 資格情報 UI は Windows 所有のものに任せ、PC Custom は秘密を受け取らない。

VPN 接続を作業用アプリ起動や窓配置と束ねる余地はあるが、接続単体の ownership と障害復旧が proof できるまで B の候補には数えない。

### 4) 使える公式 Windows API / 文書

- 登録済み RAS entry の列挙: [RasEnumEntriesW](https://learn.microsoft.com/en-us/windows/win32/api/ras/nf-ras-rasenumentriesw)
- Windows 所有の接続 UI: [RasDialDlgW](https://learn.microsoft.com/en-us/windows/win32/api/rasdlg/nf-rasdlg-rasdialdlgw)
- 接続開始: [RasDialW](https://learn.microsoft.com/en-us/windows/win32/api/ras/nf-ras-rasdialw)
- 現在の接続列挙: [RasEnumConnectionsW](https://learn.microsoft.com/en-us/windows/win32/api/ras/nf-ras-rasenumconnectionsw)
- 状態確認: [RasGetConnectStatusW](https://learn.microsoft.com/en-us/windows/win32/api/ras/nf-ras-rasgetconnectstatusw)
- 切断: [RasHangUpW](https://learn.microsoft.com/en-us/windows/win32/api/ras/nf-ras-rashangupw)、[Disconnecting a RAS Connection](https://learn.microsoft.com/en-us/windows/win32/rras/disconnecting)

初期 proof は `RasDialDlgW` を優先し、資格情報入力を Windows のダイアログ内に閉じ込める。非対話の秘密保存は検討しない。

### 5) 元に戻せるか

**同一実行中に所有を証明できる場合だけ戻せる。クラッシュ後は完全には戻せない。**

開始時に同名 entry が未接続であることを確認し、PC Custom が開始した接続の handle を保持する。同じ実行中の終了操作では、その handle と entry の接続状態を再確認してから `RasHangUpW` を呼び、切断完了まで `RasGetConnectStatusW` で待つ。

しかしアプリがクラッシュ・再起動すると、列挙された同名接続が「PC Custom が開始したもの」か、その後に利用者が再接続したものかを確実に区別できない。したがって再起動後の自動切断はしない。履歴を recovery-needed とし、現在の接続名と Windows の切断画面を示して利用者判断に委ねる。

開始前から接続済みなら「既に接続中」と表示し、終了時に切らない。

### 6) 難易度と最大のリスク

**難易度: 高。** RAS entry と現代の VPN アプリが同じものではなく、認証 UI、MFA、接続遷移、切断待ち、休止・ネットワーク切替、クラッシュ recovery を扱う必要がある。

最大のリスクは、仕事中の通信を誤って切ること。特に再起動後は ownership を証明できないため、自動 rollback の看板と相性が悪い。第二のリスクは、利用者が「あらゆる VPN に対応」と誤解すること。対象を Windows 標準 RAS entry のみに限定し、非対応 entry を曖昧に表示しない。

### 7) PC Custom の核との適合

中。必要な間だけ変え、終われば戻すという利用場面には合う。しかし durable な exact rollback がクラッシュをまたいで成立しない点は製品の核と衝突する。

そのため、現時点では一般の reversible action に昇格させず、manual-only の research card とする。同一実行中の ownership、切断完了、休止復帰を proof できた後でも、再起動後は「案内」であることを明示する必要がある。

### 8) 外から効いたとどう測るか

- テスト専用の RAS サーバーと entry を使い、別プロセスの `RasEnumConnectionsW` / `RasGetConnectStatusW` で connected 状態を確認する。
- 接続前後でテストサーバー内の固定 endpoint だけへ疎通し、接続中に到達し、切断後に到達しないことを確認する。一般インターネット速度や「安全になった」は測らない。
- OS の route / adapter 変化も補助観測するが、VPN ごとの差が大きいため、それ単独を成功条件にしない。
- 切断後は handle が無効になるまで待ち、テスト endpoint の到達不能と接続列挙からの消失を確認する。
- 接続中に PC Custom を強制終了する試験では、自動切断できないことを失敗として隠さず、再起動後に recovery-needed と現在状態を示すことを合格条件にする。

実在する企業 VPN endpoint を自動 probe したり、接続先アドレスを製品分析へ送ったりしない。

---

## 7. コントラストテーマを短時間試す

**ラベル: A**

### 1) 利用者が得る結果

文字や境界が見づらいとき、現在の見た目を失わずに高コントラスト表示を 15〜30 秒だけ試し、読みやすければ明示的に続ける。何もしなければ時間切れで元へ戻り、キーボードだけでも即時に戻せる。

これは視覚障害の診断や最適テーマの自動推薦ではない。利用者が日常的に使っているアクセシビリティ設定を勝手に切り替えない。

### 2) なぜ必要か — 実在する声

- [視覚障害があり、ほとんど常に high contrast を使うがショートカットが動かない](https://www.reddit.com/r/Blind/comments/1e76a20/) — 2024-07-19、+2。直接の利用者ニーズだが票は少なく限定的。
- [Windows 11 で high contrast の切替に困る](https://www.reddit.com/r/WindowsHelp/comments/1eam88z/) — 2024-07-23、+1。単独の低票。
- [Windows 11 で high contrast を勧める一方、タスクバーの見え方に不満](https://www.reddit.com/r/Windows11/comments/q7x31p/) — 2021-10-14、+2。テーマが全員に適するわけではないことを示す。
- [誤って high contrast を有効にし、元の見た目へ戻しにくい](https://www.reddit.com/r/WindowsHelp/comments/1s1ar95/windows_11_is_selectively_using_high_contrast_on/) — 2026-03-23、+1。自動化の危険と、安全な試用・復元の必要性を示す限定事例。

票数はすべて小さい。アクセシビリティ機能は人数だけで優先度を決めない一方、当事者の既存設定を壊すリスクも高いため、通常機能ではなく opt-in の proof 候補とする。

### 3) 競合と埋まっていない隙間

Windows 11 には [Contrast themes](https://support.microsoft.com/en-US/accessibility/windows/change-color-contrast-in-windows) があり、設定画面とキーボードショートカットで切り替えられる。[Turn high contrast mode on or off](https://support.microsoft.com/en-us/windows/turn-high-contrast-mode-on-or-off-in-windows-909e9d89-a0f9-a3a9-b993-7a6dcee85025) も公式手順を提供する。

PC Custom の隙間は、新テーマ編集ではなく「元の状態を保護した短時間の試着」である。

- 適用前に現在状態と scheme を採取する。
- 視認性が変わる前に、時間切れ復元と戻すキーを説明する。
- 何もしなければ自動で戻し、続ける場合も PC Custom の一時履歴を残す。

ただし Windows 11 の Contrast themes と legacy high-contrast API の対応が完全であると proof できない限り、任意テーマ選択や保存機能へ広げない。

### 4) 使える公式 Windows API / 文書

- 状態取得・設定: [SystemParametersInfoW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow) の `SPI_GETHIGHCONTRAST` / `SPI_SETHIGHCONTRAST`
- 構造体: [HIGHCONTRASTW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-highcontrastw)
- Windows 11 の利用者向け仕様: [Change color contrast in Windows](https://support.microsoft.com/en-US/accessibility/windows/change-color-contrast-in-windows)

`SPI_SETHIGHCONTRAST` では構造体の必要フィールドを一式渡し、設定変更通知を伴う公式フラグを使う。レジストリ直書きや undocumented theme file 操作はしない。

### 5) 元に戻せるか

**API と Windows 11 Contrast themes の対応を proof できた構成だけ、短時間試用として戻せる。現時点では一般提供を約束しない。**

適用前に `HIGHCONTRASTW` の安定して取得できる全フィールドと scheme 名を snapshot し、試用中は 15〜30 秒の countdown、常時表示の「戻す」、キーボード escape path を用意する。終了時は現在状態が PC Custom の applied value と一致する場合だけ元値を設定する。

次の場合は自動化しない。

- 開始時から high contrast / contrast theme を利用中。
- custom contrast theme の完全な round-trip が確認できない。
- 適用中に利用者または Windows が別テーマへ変更した。
- 「戻す」UI 自体が見えなくなる、またはキーボード操作できない環境。

### 6) 難易度と最大のリスク

**難易度: 高。** 呼び出し数ではなく、アクセシビリティ設定を壊さない proof、Windows 11 の modern contrast theme と legacy SPI の対応、失敗時でも見える復帰 UI が難しい。

最大のリスクは、見え方を急変させて利用者が「戻す」操作自体をできなくすること。特にこの設定を常用する利用者の scheme を失う事故は許容できない。自動プロファイル、起動時適用、ホットコーナー、無確認の timer 延長には入れず、明示 opt-in の独立画面だけにする。

### 7) PC Custom の核との適合

考え方は高く適合する。Windows の強い見た目変更を「試して、確実に戻す」設計は PC Custom らしい。

ただし実装可能性の適合は低〜中。exact rollback を証明できなければ、製品の核を守るために setter を採用せず、Windows の contrast themes 設定への案内と現在状態の表示だけに留めるべきである。

### 8) 外から効いたとどう測るか

- setter と別の新規テストプロセスに標準ボタン、入力欄、選択状態、リンク、無効状態を表示し、Windows の `SystemParameters.HighContrast` 相当の公開状態と system color の変化を確認する。
- そのテスト窓をスクリーンショット化し、既知の前景／背景ピクセルと境界が変化したこと、文字と背景の contrast ratio が期待方向へ動いたことを測る。これは「本人に読みやすい」の測定ではない。
- rollback 後に同じテスト窓を新規作成し、公開状態、system color、主要ピクセルが開始前へ戻ることを確認する。
- custom contrast theme を複数用意し、scheme 名、色、シェル、標準 Win32 control が完全に戻らない構成は不採用にする。
- 実際の読みやすさ、色覚特性への適合、すべての第三者アプリの描画は自動測定できない。利用者本人の確認が必要である。

---

## 横断評価

### 未着手領域

A を満たすのは次の五領域で、最低三領域の条件を超える。

1. ストレージの時系列観測
2. 印刷
3. 地域・時刻表示
4. ネットワーク接続
5. アクセシビリティ

Round 1 / Round 2 の wallpaper、taskbar、pointer、power、display、microphone、hot corner、appearance scene を名前だけ変えて再提案していない。

### 「まとめて戻せる」利用場面

B を満たすのは次の二つ。

1. 一時ワークスペース: 窓配置、allowlist 起動、外観、スリープ抑止などを一つの開始・終了へ束ねる。
2. 画面共有前後の準備・復帰: readiness と安全に自動変更できる既存 action を束ね、手動項目は所有しない。

どちらも「全項目が必ず戻った」という一括成功表示にはしない。子 action ごとに restored / skipped-external-change / failed / manual-check-needed を出す。

### 新しい API を増やさず成立するもの

C を満たすのは一時ワークスペースと画面共有前後の準備・復帰である。どちらも既存 action、probe、durable history、固定 Settings link の構成で最小版を出せる。

新規 API を使う候補のうち、既定プリンターと時刻表示は setter と独立観測経路が比較的明確である。VPN とコントラストテーマは、通常の reversible action として約束するには rollback の穴が大きいため、research のままにする。

## まず作るならこの3つ

### 1位: 一時ワークスペース — 始める前の机へ戻す

PC Custom の既存資産を最も強く再利用でき、action 数を増やさずに「一時変更と復帰」という製品価値を前面へ出せる。PowerToys Workspaces との差も、起動・配置ではなく「開始前へまとめて戻る」と明確である。

最初は窓配置と一つか二つの既存 action だけに絞る。アプリ自動終了を入れず、部分 rollback と外部競合の表示を先に完成させる。

### 2位: 容量がいつ減ったか分かる履歴

複数の直近ユーザー投稿で同じ困りごとが反復し、Windows 標準の現在値表示と既存 disk analyzer の「今どこが大きいか」に対して、時間差という分かりやすい隙間がある。読み取り専用なので OS 状態を壊さず、PC Custom の観測・説明責任を強くできる。

初期版はドライブ空き容量と利用者が選んだ既知フォルダー数個だけ、保存は集計値だけ、走査は厳しい budget 付きにする。原因断定と削除導線は入れない。

### 3位: 画面共有前後の準備・復帰

一度の通知露出の損失が大きく、既存の readiness、窓配置、スリープ抑止、mode ribbon を一つの実生活フローへまとめられる。新規 setter を増やさず試せるため、小さな検証版を早く出せる。

ただし「安全」バッジや通知抑止保証は出さない。自動で確認できた項目、利用者が手動確認した項目、確認できない項目を分け、自動変更したものだけを終了時に戻す。この正直な境界表示まで含めて初期版の完成条件とする。
