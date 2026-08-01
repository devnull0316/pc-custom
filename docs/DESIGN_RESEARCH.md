# PCカスタム UI/UX デザイン調査・分析報告書 (`docs/DESIGN_RESEARCH.md`)

本書は `tasks/TASK_DESIGN_RESEARCH.md` に基づき、現行プロダクト「PCカスタム（totonoe）」のデザイン構造・情報の階層・導線を分析（段階1）し、実在する各種ソフトウェア・ガイドラインの作法を調査（段階2）した上で、プロダクトの芯に沿った具象的な改善案（段階3）をまとめた調査文書である。コードの変更は行わず、設計文書の策定のみを行う。

---

## 段階1: いまのデザインを把握する

### 1.1 画面の構成と各画面の目的

本プロダクトは `src/App.tsx` の状態管理のもと、**5つの主要画面（ビュー）**、**4つのモーダル/ダイアログ**、および画面下部に常駐する**試用/Undoバー**および通知トーストで構成されている。

1. **ホーム画面 (`HomeView`, `src/components/HomeView.tsx`)**
   - **目的**: 利用者が最初に到達するダッシュボード。「結果から選ぶ」原則に基づき、現在のPC状態に応じたおすすめの提示、9つの結果タイルへの分岐、直近の変更履歴の確認を行う。
2. **Action ブラウザ画面 (`ActionBrowser`, `src/components/ActionBrowser.tsx`)**
   - **目的**: 68項目以上の個別設定（Action）をカテゴリ別に参照・個別検証・適用・プロファイル下書きへ追加するためのメインカタログ画面。左側にカテゴリと項目一覧、右側に選択項目の詳細ペイン（現在値/適用後/リスク/更新影響/戻し方など）を配する。
3. **モード画面 (`ProfilesView`, `src/components/ProfilesView.tsx`)**
   - **目的**: 「ゲーム」「勉強」「作業」など利用場面に合わせ、複数Actionの組み合わせ（プロファイル）を作成・管理・実行する画面。プロセス起動検知や手動トリガーによる自動適用/復元を管理し、「VALORANTが始まったら…」のような自然言語での挙動要約を表示する。
4. **PCセットアップ画面 (`SetupView`, `src/components/SetupView.tsx`)**
   - **目的**: OSインストール直後や新PC導入時の初期設定を集約する画面。「Windowsの仕上げ」「普段使いの機能（PowerToys連携）」「アプリ導入」の3タブで構成。
5. **タイムライン画面 (`TimelineView`, `src/components/TimelineView.tsx`)**
   - **目的**: 適用されたすべての設定変更を時系列で監査・管理する画面。項目単位での個別復元（rollback）や、外部競合・復旧が必要な項目の再検証を行う。

#### ダイアログ / モーダル・補助UI
- **コマンドパレット (`CommandPalette`, `src/components/CommandPalette.tsx`)**: `Ctrl + K` で起動。全画面・全Actionへの即時検索・ジャンプを提供。
- **適用プレビューダイアログ (`Dialog`, `src/components/App.tsx` L656-L670)**: 設定変更を最終決定する前に、現在値と適用後の差分・注意事項を明示する必須確認モーダル。
- **ロールバック確認ダイアログ (`Dialog`, `src/components/App.tsx` L671-L680)**: タイムラインから特定項目を復元する前の確認モーダル。
- **モードの下書きダイアログ (`Dialog`, `src/components/App.tsx` L681-L690)**: 一時的に選択したActionをプールし、後でモードとして保存するためのスクラッチパッド。
- **Undoバー (`undo-bar`, `src/components/App.tsx` L691-L714)**: 適用直後に画面下部に浮遊表示される通知・試用タイマー（30秒）バー。ワンクリックでの即時元戻しと試用保存を提供する。

---

### 1.2 情報の階層（何が主で何が従か）

本プロダクトのUIは、初心者が専門知識なしで安全に操作できるよう、厳格な情報階層が組まれている。

- **主（プライマリ要素）**:
  - **得られる結果（ユーザー言語のタイトル）**: レジストリ名や内部IDではなく、「タスクバーの時計に秒を表示する」「ゲーム中のShift確認画面を止める」といった平易な結果表現。
  - **現在と適用後の状態比較（Before / After）**: 現在Windowsがどうなっており、適用後にどう変わるかの明示。
  - **一発復元（Undo / Rollback）**: 適用直後の「元に戻す」ボタン、タイムラインの「この変更だけ戻す」アクション。
- **従（セカンダリ・サポーティブ要素）**:
  - **属性バッジ/チップ**: 危険度（`低リスク` / `注意` / `実験的`）、管理者権限の要否（`管理者権限が必要`）、再起動要否、Windows Update影響（`Update影響: 低い`など）。
  - **内部識別子および技術説明**: `session.prevent_sleep` などのAction ID、`SetThreadExecutionState` などの内部API/レジストリパス（「詳細を見る」折りたたみや問い合わせ用情報内に秘匿）。

---

### 1.3 色・文字サイズ・余白のトークン定義と使用状況 (`src/styles.css`)

デザインシステムは `src/styles.css` に集約されており、Windows 11 Fluent Design（Mica素材）との調和とアクセシビリティ（WCAG AA）を意識してトークン化されている。

- **色（カラーパレット）**:
  - **面・背景**: `--canvas` (ライト: `rgba(242,243,245,0.92)` / ダーク: `rgba(18,18,18,0.86)`), `--surface` (半透明Mica風), `--sidebar`.
  - **アクセント**: OSのシステムアクセントカラーに自動追従 (`@supports (color: AccentColor)`). フォールバックは青緑系 (`#2FA6A0`). ※文字色には使用せず背景・枠線・フォーム部品にのみ適用。
  - **危険度/状態**:
    - 安全 (Safe): `--safe: #177245` (ダーク: `#79c99d`)
    - 注意 (Caution): `--caution: #8a4b00` (ダーク: `#f2bb62`)
    - 実験的 (Experimental): `--experimental: #853b4b` (ダーク: `#ec9bad`)
    - 危険/エラー (Danger): `--danger: #b42318` (ダーク: `#ff9b91`)
- **文字サイズ（タイポグラフィ）**:
  - `Segoe UI Variable Text` を第一候補として指定。過去の監査（2026-07-27）により16種類あったフォントサイズを以下の**7段階のトークン**に完全固定・一元化。
  - `--fs-xs` (12px): 補助ラベル、チップ
  - `--fs-sm` (13px): 説明文、二次情報
  - `--fs-base` (14px): 本文
  - `--fs-md` (16px): 小見出し、強調本文
  - `--fs-lg` (19px): セクション見出し
  - `--fs-xl` (24px): 画面タイトル
  - `--fs-display` (30px): ディスプレイ見出し
- **余白・角丸・影**:
  - カード角丸: `--radius-card: 8px`
  - コントロール角丸: `--radius-control: 6px`
  - 余白: 4pxグリッドに準拠。ビュー領域は `padding: 32px 36px 44px`.

---

### 1.4 繰り返し現れる部品の型

1. **結果タイル (`.result-tile`, `HomeView.tsx`)**: アイコン、タイトル、1行説明文、矢印アイコンで構成されるカード（ホーム画面に均一3列グリッドで配置）。
2. **Action行・マスター項目 (`.action-row`, `ActionBrowser.tsx`)**: 左側リストに並ぶ選択項目。状態シンボル、タイトル、説明文、危険度ラベルが収まる。
3. **詳細ペイン (`.action-detail`, `ActionBrowser.tsx`)**: 右側に固定表示。タイトル、対象ユーザー、危険度/権限バッジ、現在値 vs 適用後の対照ボックス、変更手段の説明、プレビュー/下書き追加ボタンで構成。
4. **コールアウト / バナー (`.error-banner`, `.recovery-callout`, `.notice-toast`, `.undo-bar`)**: 注意喚起、復旧要求、通知、復元用フローティングバー。
5. **空状態 (`.compact-empty`, `.dialog-empty`)**: データが0件の際、アイコン＋太字メッセージ＋案内文で次の行動を示す。

---

### 1.5 利用者の導線

- **初回起動（初めて開く）**:
  - `HomeView` に到着。Windowsのビルド確認結果と「安全コア接続中」ステータスを確認。検出されたPC状態に基づく「いまのPCを見て、おすすめ」および9つの「結果タイル」から希望するゴールを選択。
- **設定を変更する（何かを変える）**:
  - 結果タイルまたはナビゲーション「変更する」から `ActionBrowser` に移動。
  - 上部のカテゴリバーまたは矢印キー（`↑` `↓`）で項目を辿り、右ペインで「現在」と「適用後」を確認。
  - 「適用プレビュー」をクリック $\rightarrow$ 確認ダイアログで差分と注意事項を最終チェックし「確認して適用」を実行。
  - 画面下部に `UndoBar` が出現（30秒の試用タイマーがカウントダウン開始）。
- **元に戻す（戻す）**:
  - **直後の場合**: `UndoBar` の「元に戻す」を1クリック。
  - **後からの場合**: サイドバーから `TimelineView` へ移動 $\rightarrow$ 履歴一覧から対象項目を選択し「この変更だけ戻す」を実行。

---

### 1.6 `docs/DESIGN_LANGUAGE.md` の確認

**`docs/DESIGN_LANGUAGE.md` は存在する。**
本文書には、Linear（静かな高密度・コマンドパレット）、Raycast（結果ファースト＋右詳細ペイン・キーボード追従）、Apple Shortcuts（文章による自動要約）、Steam/Playnite（ゲームプロファイル）、1Password/Proton（透明性・安全設計）からの借用作法が明確に定義されている。
また、過去のCC実測（2026-07-25〜07-28）により、コントラストAA準拠、Mica上の明度差調整、キーボードナビゲーション（`ArrowUp/Down`）、フォントサイズ16種から7種への統一などの改善履歴が記録されており、現行コードの CSS やコンポーネント設計へ直接反映されている。

---

## 段階2: 外を調べる（web検索結果と実在の作法）

本プロダクトの芯である**「Windowsの設定を安全に変え、1件ずつ元へ戻せる」**価値を補強するため、類似の性質を持つ実在製品・ガイドラインの設計作法をweb検索により調査した。

---

### 2.1 OSの設定を変えるツールの UI 作法

1. **Microsoft Fluent Design System & Windows App Design Guidelines**
   - **参照URL**: [Fluent 2 Design System](https://fluent2.microsoft.design/) / [Windows App Design Guidelines](https://learn.microsoft.com/en-us/windows/apps/design/) (2026年8月1日参照)
   - **作法**:
     - 多数の設定項目を整理する **Progressive Disclosure（段階的開示）** の採用。
     - 4px単位のスペーシングランプと、標準的なトグル・コンボボックス・Mica/Acrylicによる層（Elevation）構造。
     - 設定は「変更結果が即座に反映される」メンタルモデルを基本としつつ、影響が大きい変更にはレイヤー化された詳細ビューを提供する。
2. **Apple Human Interface Guidelines (macOS Settings / Preferences)**
   - **参照URL**: [macOS Settings Guidelines](https://developer.apple.com/design/human-interface-guidelines/settings) (2026年8月1日参照)
   - **作法**:
     - **"Minimize Settings"（設定を最小化する）**: ユーザーが手動調整しなくても済む適切な既定値（Defaults）を優先する。
     - ツールバーやサイドバーによる固定ナビゲーションと、直前に開いていたペインの状態保持（State Restoration）。
3. **Microsoft PowerToys UX Guidelines**
   - **参照URL**: [PowerToys Documentation](https://learn.microsoft.com/en-us/windows/powertoys/) (2026年8月1日参照)
   - **作法**:
     - 全体設定（管理者モード・バックアップ・復元）と個別ユーティリティの分離。
     - Command Palette（PowerToys Run）による一覧操作とフォーム/リストの統合パターン。

---

### 2.2 「元に戻せる」ことを中心に据えた製品の見せ方

1. **Adobe Lightroom & Photoshop（非破壊編集・ヒストリー）**
   - **参照URL**: [Lightroom Basic Concepts & Non-Destructive Editing](https://helpx.adobe.com/lightroom-classic/help/lightroom-basic-concepts.html) (2026年8月1日参照)
   - **作法**:
     - **「原版は聖域（Original is Sacred）」パターン**: 元の画像・ファイルには一切上書き変更を加えず、変更パラメータのみを保持するパラメトリック編集。
     - **非線形ヒストリー (History Panel)**: 過去の任意の時点（スナップショット）へいつでもジャンプして復元できるUI。直線的な Undo/Redo の消滅不安を排除。
2. **Git & GitHub Desktop（バージョン管理・ロールバック）**
   - **参照URL**: [GitHub Desktop Documentation](https://desktop.github.com/) / [Git Reference](https://git-scm.com/doc) (2026年8月1日参照)
   - **作法**:
     - 変更前後の差分（Diff）を視覚的に提示した上でコミット/ロールバックを実行。
     - 単一コミットの取り消し（Revert）とタイムラインからの過去状態復元。
3. **Database Migration Tools (Prisma Migrate, Flyway, Hasura)**
   - **参照URL**: [Prisma Migrate Docs](https://www.prisma.io/docs/concepts/components/prisma-migrate) / [Flyway Documentation](https://flywaydb.org/documentation/) (2026年8月1日参照)
   - **作法**:
     - **Diff-First Preview**: 適用前に `migrate diff` 相当の対照表を提示。
     - **トランザクション内実行とアトミックロールバック**: 適用途中に失敗した場合、自動的に変更前の状態へ巻き戻す。
     - 変更不可のマイグレーション履歴ログ（`flyway_schema_history` 相当の透過的保持）。

---

### 2.3 危険度のある操作を初心者に見せる作法

1. **Nielsen Norman Group (NN/G) 破壊的操作・確認ダイアログのガイドライン**
   - **参照URL**: [Confirmation Dialogs: When to Use Them and When to Avoid Them (NN/G)](https://www.nngroup.com/articles/confirmation-dialogs/) (2026年8月1日参照)
   - **作法**:
     - **Confirmation Fatigue（確認疲れ）の防止**: 日常的・軽微な操作でいちいちポップアップを出さず、後から「Undo（元に戻す）」できるUIを優先する。
     - **Action-Oriented Microcopy**: 「よろしいですか？［はい／いいえ］」ではなく、「『プロジェクトを削除』［キャンセル／削除する］」のように動詞＋名詞の具体的なボタン名にする。
     - **Active Friction for High-Risk**: データベース削除などの極めて高危険度な操作では、リソース名の手入力や明確な確認チェックボックスを要求して反射的クリックを防ぐ。
2. **Stripe & AWS インフラ管理画面の安全設計**
   - **参照URL**: [Stripe API Developer Docs](https://stripe.com/docs/keys) / [AWS Management Console Guidelines](https://docs.aws.amazon.com/) (2026年8月1日参照)
   - **作法**:
     - **Test / Live Mode スイッチ**: 本番環境に影響を与えないサンドボックス・ドライラン（Dry-Run）環境の明示。
     - **影響範囲（Impact Assessment）の可視化**: 実行によって影響を受けるリソース数や副作用を適用前に一覧で明示する。

---

### 2.4 項目数が多い設定画面の整理のしかた

1. **VS Code Settings UI & Google Chrome / Mozilla Firefox 設定**
   - **参照URL**: [VS Code User Settings Topic](https://code.visualstudio.com/docs/getstarted/settings) / [Chrome Settings Support](https://support.google.com/chrome) (2026年8月1日参照)
   - **作法**:
     - **即時リアルタイム検索 (Search-First)**: 設定項目名だけでなく、キーワード・類義語・関連タグをインデックス化した高速検索入力。
     - **Progressive Disclosure（段階的開示）**: 高頻度で使われる設定を前列に配置し、高度な設定・専門的設定は「詳細」やアコーディオン内に集約。
     - **カテゴリーツリーと固定ヘッダー**: スクロール位置に応じて現在位置を見失わないステータス表示。

---

## 段階3: 改善案（7件）

プロダクトの芯である**「Windowsの設定を安全に変え、1件ずつ元へ戻せる（初心者・ゲーマー向け）」**に合致するもの厳選7件を挙げる。「モダンにする」「洗練させる」といった曖昧な記述は排除し、具体的なファイル・行番号・引用・変更案・根拠を記述する。

---

### 改善案1: ActionBrowser 一覧へのリアルタイムキーワード・タグ検索入力の追加

- **1. 何が良くなるか（利用者の言葉で）**:
  70件近くある設定の中から、「時計」「スリープ」「拡張子」「ゲーム」などの言葉を入力するだけで、目的の設定項目を1秒で見つけられるようになる。
- **2. いまどうなっていて、何が問題か**:
  - **対象ファイル**: `src/components/ActionBrowser.tsx`
  - **該当行**: L119-L139
  - **引用**:
    ```tsx
    119: <div aria-label="種類でしぼる" className="category-bar">
    120:   {CATEGORIES.map((category) => {
    ```
  - **問題点**: コマンドパレット（`Ctrl + K`）は存在するものの、主画面である `ActionBrowser` 内にはテキスト検索窓が存在せず、カテゴリボタン（9個）を1つずつ押して目で探すしかない。70件近い設定がある画面において、カテゴリ絞り込みのみでは探索コストが高い。
- **3. どう変えるか（具体的に）**:
  - `ActionBrowser.tsx` の `category-bar` 直上に `.action-search-input` フィールドを追加。
  - 検索状態 `searchQuery` を保持し、Actionの `name`, `description`, `tags`, `desiredState` に対するリアルタイムフィルタリングを適用。
  - **CSS値**: `height: 38px`, `padding: 0 12px 0 36px`, `border-radius: var(--radius-control)`, `background: var(--surface-muted)`, `font-size: var(--fs-base)`. 検索アイコンを左端に配置。
- **4. 根拠**:
  - VS Code Settings UI および Google Chrome 設定の Search-First Navigation 作法。
  - **URL**: [VS Code User Settings](https://code.visualstudio.com/docs/getstarted/settings) (2026年8月1日参照)
- **5. 実装の大きさと壊しうるもの**:
  - **大きさ**: 小（コンポーネント内ローカル状態と `filter` 処理のみ）。
  - **壊しうるもの**: 矢印キー移動（`onKeyDown`）のフォーカス管理が検索入力時に入力文字と干渉しないよう制御が必要。
- **6. やらない理由があるならそれも書け**:
  - 検索窓の追加は利便性向上に直結し、やらない理由はない。

---

### 改善案2: 適用プレビューダイアログにおける「注意・実験的」リスク理由と副作用の明示

- **1. 何が良くなるか（利用者の言葉で）**:
  「注意」や「実験的」と書かれた設定を適用する際、具体的にどのようなリスク（例: Shiftキーの連打機能が使えなくなる等）があるのかを適用前に理解でき、安心して試せるようになる。
- **2. いまどうなっていて、何が問題か**:
  - **対象ファイル**: `src/App.tsx`
  - **該当行**: L664-L666
  - **引用**:
    ```tsx
    664: <span className={`risk-label risk-label--${change.riskLevel}`}>
    665:   {change.riskLevel === "safe" ? "低リスク" : change.riskLevel === "caution" ? "注意" : "実験的"}
    666: </span>
    ```
  - **問題点**: 適用プレビューダイアログにおいて「注意」「実験的」というラベルは表示されるが、なぜ注意なのか、具体的にどんな副作用があるのかのテキストが表示されていないため、初心者が不安を感じるか、あるいはリスクを見落として適用してしまう。
- **3. どう変えるか（具体的に）**:
  - `PreviewChange` に含まれる Action の `audience`（「〜には向きません」）や `detailPoints` をプレビューダイアログ項目内へ抽出表示。
  - `riskLevel === "caution" || riskLevel === "experimental"` の場合、警告コールアウトボックス（`.preview-warnings`）内に「【注意点】〜」としてリスク理由を明示し、チェックボックスの同意文言を動的に変更。
- **4. 根拠**:
  - NN/G の High-Risk Operations & Action-Oriented Microcopy ガイドライン（漠然とした確認ではなく、具体的リスクと影響範囲を伝える）。
  - **URL**: [Confirmation Dialogs (NN/G)](https://www.nngroup.com/articles/confirmation-dialogs/) (2026年8月1日参照)
- **5. 実装の大きさと壊しうるもの**:
  - **大きさ**: 小（ダイアログ内のレンダリングロジック拡充）。
  - **壊しうるもの**: なし。
- **6. やらない理由があるならそれも書け**:
  - なし。

---

### 改善案3: タイムライン一覧での変更前後の差分（Before / After）常時可視化

- **1. 何が良くなるか（利用者の言葉で）**:
  過去の変更履歴を見るとき、どの設定が「何から何へ変わったか（例: 非表示 $\rightarrow$ 表示）」が一覧ですぐ分かり、戻したい変更を迷わず選べるようになる。
- **2. いまどうなっていて、何が問題か**:
  - **対象ファイル**: `src/components/TimelineView.tsx`
  - **該当行**: L115-L125
  - **引用**:
    ```tsx
    115: <span className="timeline-item__copy">
    116:   <strong>{item.title}</strong>
    117:   <small>{item.summary}</small>
    118: </span>
    ```
  - **問題点**: タイムラインの各項目にはタイトルとサマリーテキストのみが表示され、具体的な `before` （変更前）と `after` （変更後）の値が詳細ダイアログを開くまで見えないため、複数回変更した際に履歴の識別がしにくい。
- **3. どう変えるか（具体的に）**:
  - タイムラインの各行 (`.timeline-item`) 内に、変更前後の対照バッジ (`.timeline-diff-chip`) を常時表示。
  - **構成**: `[ 変更前: {item.before} ] → [ 変更後: {item.after} ]`
  - **CSS値**: `font-size: var(--fs-xs)`, `background: var(--surface-muted)`, `padding: 2px 8px`, `border-radius: var(--radius-control)`, `color: var(--text-secondary)`.
- **4. 根拠**:
  - Git GUI（GitHub Desktop）および Prisma Migrate の Diff-First インスペクション作法（変更内容の透明性向上）。
  - **URL**: [GitHub Desktop](https://desktop.github.com/) (2026年8月1日参照)
- **5. 実装の大きさと壊しうるもの**:
  - **大きさ**: 小（タイムライン要素へのバッジ描画追加）。
  - **壊しうるもの**: なし（長文テキスト時の溢れを防ぐため `text-overflow: ellipsis` を指定）。
- **6. やらない理由があるならそれも書け**:
  - なし。

---

### 改善案4: モード一括実行前のプレビュー＆個別除外機能の追加

- **1. 何が良くなるか（利用者の言葉で）**:
  ゲームモードや勉強モードを起動する際、一括適用される予定の設定一覧を事前チェックし、「今回はこの1件だけ適用から外す」といった柔軟な調整ができるようになる。
- **2. いまどうなっていて、何が問題か**:
  - **対象ファイル**: `src/components/ProfilesView.tsx` & `src/App.tsx`
  - **該当行**: `src/App.tsx` L557-L569
  - **引用**:
    ```tsx
    561: const result = await runProfileNow(id);
    562: setNotice([result.message, ...result.details].join(" "));
    ```
  - **問題点**: モード画面で「いま実行」を押すと、プレビュー確認ダイアログを挟まずに即座に全Actionが一括適用されるため、「何が適用されたか」「特定の設定だけ除外したい」というニーズに対応できず、安全性の原則に一歩届いていない。
- **3. どう変えるか（具体的に）**:
  - 「いま実行」ボタン押下時に `previewActions` を呼び出し、モードに含まれるAction群をまとめた「適用プレビュー」ダイアログを表示。
  - ダイアログ内で各Actionのチェックボックスをオン/オフ切り替え可能にし、必要なActionのみを選択実行できるようにする。
- **4. 根拠**:
  - Apple Shortcuts のステップ別実行確認および Flyway のマイグレーション選択実行作法。
  - **URL**: [macOS Human Interface Guidelines](https://developer.apple.com/design/human-interface-guidelines/settings) (2026年8月1日参照)
- **5. 実装の大きさと壊しうるもの**:
  - **大きさ**: 中（`ProfilesView` から `App.tsx` のプレビューフローへの接続）。
  - **壊しうるもの**: ワンクリック実行の手間が1ステップ増えるため、ダイアログに「次回から確認せず実行」オプションを設ける配慮が推奨される。
- **6. やらない理由があるならそれも書け**:
  - 素早い実行を重視するユーザーには手間に感じられる可能性があるが、「安全性を最優先する」プロダクトの芯に照らせばプレビュー挟み込みが望ましい。

---

### 改善案5: 試用タイマー（30秒）の視覚的プログレスバーゲージの追加

- **1. 何が良くなるか（利用者の言葉で）**:
  設定を変更した直後、あとどれくらいの時間で自動的に元に戻るのかがプログレスバーの動きで直感的にわかり、焦らず試せるようになる。
- **2. いまどうなっていて、何が問題か**:
  - **対象ファイル**: `src/App.tsx`
  - **該当行**: L691-L701
  - **引用**:
    ```tsx
    697: ? `試しに適用しています（保存しなければ元に戻ります・残り${trialLeft}秒）`
    698: : `見え方を試しています（自動的に元へ戻す・残り${trialLeft}秒）`}
    ```
  - **問題点**: 画面下部の `UndoBar` に「残り15秒」とテキストで数値が表示されるだけであり、タイマーの減少が視覚的なゲージでフィードバックされないため、放置で元に戻る感覚が直感的に伝わりづらい。
- **3. どう変えるか（具体的に）**:
  - `.undo-bar` の上端または背景に、30秒から0秒へ向けてスムースに縮小する進捗バー (`.trial-progress-gauge`) を配置。
  - **CSS値**: `height: 3px`, `background: var(--accent)`, `transition: width 0.5s linear`, `position: absolute; top: 0; left: 0`.
- **4. 根拠**:
  - Windows / macOS のディスプレイ解像度変更時の「15秒後に自動復元」タイマーUIパターン。
  - **URL**: [Fluent 2 Motion & Feedback](https://fluent2.microsoft.design/) (2026年8月1日参照)
- **5. 実装の大きさと壊しうるもの**:
  - **大きさ**: 小（CSSと `trialLeft` に連動するスタイル計算のみ）。
  - **壊しうるもの**: なし。
- **6. やらない理由があるならそれも書け**:
  - なし。

---

### 改善案6: ホーム画面における「おすすめ」と「9つの結果タイル」の視覚的優先度メリハリ強化

- **1. 何が良くなるか（利用者の言葉で）**:
  アプリを開いたとき、自分のPC状態に合わせた「おすすめ設定」が一番に目に入り、どこから触り始めればよいか迷わなくなる。
- **2. いまどうなっていて、何が問題か**:
  - **対象ファイル**: `src/components/HomeView.tsx`
  - **該当行**: L72-L80
  - **引用**:
    ```tsx
    73: {RESULT_TILES.map((tile, index) => (
    74:   <button className={`result-tile result-tile--${index + 1}`} key={tile.id} ...
    ```
  - **問題点**: ホーム画面に並ぶ 9つの `RESULT_TILES` がすべて均一なカードサイズ・デザインで敷き詰められており、初心者にとって「まずどれを押せばいいか」の視覚的プライマリ・セカンダリの強調差が弱い。
- **3. どう変えるか（具体的に）**:
  - 検出結果に基づく「おすすめ (`.recommend`)」セクションを視覚的プライマリ（大きいカード・アクセント背景）として最上部に強調配置。
  - 9つの `RESULT_TILES` のうち高頻度利用トップ3（「操作と普段使い」「ゲーム前準備」「見た目」）と、その他（セカンダリ）でカードのサイズ・面スタイルに差をつける。
- **4. 根拠**:
  - Microsoft Fluent Design の Elevation & Progressive Disclosure 原則。
  - **URL**: [Fluent 2 Layout & Elevation](https://fluent2.microsoft.design/) (2026年8月1日参照)
- **5. 実装の大きさと壊しうるもの**:
  - **大きさ**: 中（ホーム画面のCSSグリッドレイアウトおよびコンポーネント構造調整）。
  - **壊しうるもの**: レスポンシブ幅でのグリッド崩れ（`minmax` 調整を慎重に行う必要あり）。
- **6. やらない理由があるならそれも書け**:
  - なし。

---

### 改善案7: 外部競合検出時の復旧コールアウト文言の初心者向け平易化

- **1. 何が良くなるか（利用者の言葉で）**:
  Windowsの設定が手動で変えられて一時停止した際、何が起きたのかと「どうすれば安全か」が専門用語なしで理解できるようになる。
- **2. いまどうなっていて、何が問題か**:
  - **対象ファイル**: `src/components/HomeView.tsx`
  - **該当行**: L34-L40
  - **引用**:
    ```tsx
    37: <h2 id="recovery-title">{bootstrap.recoveryCount > 0 ? `${bootstrap.recoveryCount}件の復旧を確認してください` : "このWindows buildでは変更を停止しています"}</h2>
    ```
  - **問題点**: 「復旧を確認してください」というシステム用語が先行しており、初心者が「PCが壊れたのではないか」と不要な不安を感じてしまう恐れがある。
- **3. どう変えるか（具体的に）**:
  - コールアウトのタイトル文言を平易化:
    - 旧: `1件の復旧を確認してください`
    - 新: `Windowsの設定が直接変更されたため、安全のため自動変更を停止しています`
  - 説明文に「『状態を確認する』を押すと、手動で変更された内容を確認し、安全に元の状態へ戻せます」という安心感を与える補足を追加。
- **4. 根拠**:
  - 1Password / Proton / AWS の Fail-Safe 情報設計パターン（安全停止の理由と回復手順の人間的説明）。
  - **URL**: [AWS Management Console Guidelines](https://docs.aws.amazon.com/) (2026年8月1日参照)
- **5. 実装の大きさと壊しうるもの**:
  - **大きさ**: 小（テキストコピーの差し替えのみ）。
  - **壊しうるもの**: なし。
- **6. やらない理由があるならそれも書け**:
  - なし。

---

## まず直すならこの3つ

本プロダクトの芯（**「Windowsの設定を安全に変え、1件ずつ元へ戻せる」**）を最も強力に高め、ユーザー体験のボトルネックを直ちに解消するための最優先改善TOP 3を以下に提示する。

### 第1位: ActionBrowser 一覧へのリアルタイムキーワード検索の追加 (改善案1)
- **理由**:
  現在、68項目以上のActionが存在するにもかかわらず、主画面である `ActionBrowser` にテキスト検索窓がなく、ユーザーは9つのカテゴリを1つずつ手動で切り替えて目視確認するしかありません。VS CodeやChromeの作法に倣い、即時検索窓を置くことで、利用者が「時計」「スリープ」「拡張子」といった言葉から0秒で目的の安全変更にアクセスできるようになります。プロダクトの基本操作性を引き上げる最優先施策です。

### 第2位: 適用プレビューダイアログにおける「注意・実験的」リスク理由の明示 (改善案2)
- **理由**:
  本プロダクトの核心価値は「初心者が安心して安全に設定を変えられること」です。現状のプレビューダイアログでは「注意」というラベルが出るだけで具体的なリスク（副作用）が書かれていないため、ユーザーが恐怖感を持つか、逆に危険性を見落とす原因になっています。NN/Gの破壊的操作ガイドラインに倣い、具体的なリスク理由と影響範囲を明示することは、プロダクトの信頼性を確立する上で不可欠です。

### 第3位: タイムライン一覧での変更前後の差分（Before / After）常時可視化 (改善案3)
- **理由**:
  「1件ずつ正確に元に戻せる」という約束を守るためには、タイムライン画面で「過去に何がどう変わったか」を一目で把握できる必要があります。現状はタイトルと概要文しか見えず、具体的な変更前後の値を見るにはダイアログを開く必要があります。GitやPrismaの作法である Before / After のインライン差分表示を常時展開することで、迷わずに正しい復元操作を行えるようになります。
