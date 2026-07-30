import { openWindowsSettings } from "../backend";
import { useState } from "react";

import { CATEGORIES } from "../catalog";
import type {
  ActionPresentation,
  BootstrapStatus,
  CategoryId,
  DataMode,
} from "../model";
import { isMutationAllowed, riskLabel } from "../model";
import { detailPointsForScreen, methodSummaryForScreen, screenText } from "../publicCopy";
import { AppearanceScenesPanel } from "./AppearanceScenesPanel";
import { ExplorerRestartPanel } from "./ExplorerRestartPanel";
import { Icon } from "./Icon";
import { StorageHistoryPanel } from "./StorageHistoryPanel";
import { TempCleanupPanel } from "./TempCleanupPanel";
import { TaskbarAutoHidePanel } from "./TaskbarAutoHidePanel";
import { ThemeSchedulePanel } from "./ThemeSchedulePanel";

interface ActionBrowserProps {
  actions: readonly ActionPresentation[];
  selectedCategory: CategoryId;
  selectedActionId: string | null;
  dataMode: DataMode;
  bootstrap: BootstrapStatus | null;
  detectionPendingId: string | null;
  previewPendingId: string | null;
  draftActionIds: ReadonlySet<string>;
  onSelectCategory: (category: CategoryId) => void;
  onSelectAction: (actionId: string) => void;
  onDetect: (actionId: string) => void;
  onPreview: (action: ActionPresentation, parameterOverride?: Record<string, string>) => void;
  onPreviewScene: (sceneId: string) => void;
  onAddToDraft: (action: ActionPresentation) => void;
  onError: (error: unknown) => void;
}

function updateImpactLabel(impact: ActionPresentation["updateImpact"]): string {
  if (impact === "low") return "Update影響: 低い";
  if (impact === "review") return "Update後: 再検証";
  return "Update影響: 高い";
}

function methodClassLabel(methodClass: ActionPresentation["methodClass"]): string {
  if (methodClass === "public_api") return "Windowsの公開された仕組み";
  if (methodClass === "microsoft_cli") return "Microsoftの標準機能";
  if (methodClass === "winget") return "Windowsのアプリ導入機能";
  if (methodClass === "official_module") return "Microsoftの標準機能";
  if (methodClass === "documented_registry") return "Windowsが保存する設定";
  if (methodClass === "limited_external") return "検証済み限定連携";
  if (methodClass === "unverified_storage") return "根拠確認中の保存情報（読み取りのみ）";
  return "分類情報なし";
}

function availabilityLabel(action: ActionPresentation): string {
  if (action.kind === "guided" && action.methodClass === "unverified_storage") {
    return "設計候補・読取のみ";
  }
  if (action.kind === "guided") return "Windows設定案内";
  if (action.availability === "mutable") return "適用可能";
  if (action.availability === "read_only") return "読み取り専用";
  if (action.availability === "detect_only") return "検出のみ";
  return "この環境では停止";
}

function blockReason(
  dataMode: DataMode,
  bootstrap: BootstrapStatus | null,
  action: ActionPresentation,
): string {
  if (dataMode !== "live") return "安全コアへ接続できていないため変更できません。";
  if (bootstrap?.mode === "recovery_required") return "未復元項目があるため、新しい変更より復旧を優先します。";
  if (bootstrap?.mode !== "ready") return bootstrap?.message ?? "Windows互換性を確認できません。";
  if (action.availability !== "mutable") return "このActionはOSを変更せず、状態だけを確認します。";
  return "";
}

export function ActionBrowser({
  actions,
  selectedCategory,
  selectedActionId,
  dataMode,
  bootstrap,
  detectionPendingId,
  previewPendingId,
  draftActionIds,
  onSelectCategory,
  onSelectAction,
  onDetect,
  onPreview,
  onPreviewScene,
  onAddToDraft,
  onError,
}: ActionBrowserProps) {
  const categoryActions = actions.filter((action) => action.category === selectedCategory);
  const selected = actions.find((action) => action.id === selectedActionId) ?? categoryActions[0];

  return (
    <div className="view action-view">
      <header className="view-heading">
        <div>
          <p className="eyebrow">ひとつずつ選ぶ</p>
          <h1>何が変わるかを先に確認</h1>
          <p>左で結果を選び、右で現在の状態、適用後、戻し方まで確認できます。</p>
        </div>
      </header>
      {/* カテゴリは横一列にする。以前は左の列の8割をこれが占め、
          肝心のAction一覧が最下部に押し込まれていた。下で選んで上のボタンへ戻る、
          という往復はこの配置が作っていた。 */}
      <div aria-label="種類でしぼる" className="category-bar">
        {CATEGORIES.map((category) => {
          const count = actions.filter((action) => action.category === category.id).length;
          return (
            <button
              aria-pressed={selectedCategory === category.id}
              className="category-button"
              key={category.id}
              onClick={() => onSelectCategory(category.id)}
              type="button"
            >
              <span><Icon name={category.icon} /></span>
              <span className="category-chip__copy">
                <strong>{category.label}</strong>
                <small>{category.description}</small>
              </span>
              <span className="category-count">{count}</span>
            </button>
          );
        })}
      </div>
      <div className="action-workspace">
        <div className="action-master">
          <div
            aria-label="変更できる項目の一覧"
            className="action-list"
            onKeyDown={(event) => {
              // Raycast や Linear と同じく、一覧は上下キーだけで辿れるようにする。
              // 選択が動くと右の詳細も追従するので、手をキーボードから離さずに読める。
              const keys = ["ArrowDown", "ArrowUp", "Home", "End"];
              if (!keys.includes(event.key) || categoryActions.length === 0) return;
              event.preventDefault();
              const current = categoryActions.findIndex((item) => item.id === selected?.id);
              const last = categoryActions.length - 1;
              const next =
                event.key === "Home" ? 0
                : event.key === "End" ? last
                : event.key === "ArrowDown" ? Math.min(current + 1, last)
                : Math.max(current - 1, 0);
              const target = categoryActions[next];
              if (target === undefined || target.id === selected?.id) return;
              onSelectAction(target.id);
              const row = event.currentTarget.querySelectorAll<HTMLButtonElement>(".action-row")[next];
              row?.focus();
              row?.scrollIntoView({ block: "nearest" });
            }}
          >
            <p className="list-label">
              <span>このカテゴリ</span>
              {/* 矢印で辿れることは、言われなければ誰も気づかない。
                  コマンドパレットと同じようにヒントを出す（Raycast/Linear の作法）。 */}
              <span className="list-label__hint"><kbd>↑</kbd><kbd>↓</kbd>で移動</span>
            </p>
            {categoryActions.length === 0 ? (
              <div className="inline-empty"><Icon name="action" /><p><strong>この種類には項目がありません</strong>別の種類を選んでください。</p></div>
            ) : categoryActions.map((action) => (
              <button
                aria-current={selected?.id === action.id ? "true" : undefined}
                className="action-row"
                key={action.id}
                onClick={() => onSelectAction(action.id)}
                type="button"
              >
                <span className={`risk-rail risk-rail--${action.riskLevel}`} />
                <span className="action-row__copy">
                  <strong>{action.name}</strong>
                  <small>{action.currentState == null ? "現在の状態を確認" : screenText(action.currentState.label, "Windowsから読み取った状態")}</small>
                </span>
                <span className="action-row__kind">{availabilityLabel(action)}</span>
                <Icon name="chevron" size={16} />
              </button>
            ))}
          </div>
        </div>
        <section aria-live="polite" className="action-detail" key={selected?.id ?? "empty"}>
          {selected === undefined ? (
            <div className="detail-empty"><Icon name="action" size={26} /><h2>左から項目を選んでください</h2><p>現在の状態と、変えたあとの状態をここに並べます。</p></div>
          ) : (
            <ActionDetail
              action={selected}
              bootstrap={bootstrap}
              dataMode={dataMode}
              detecting={detectionPendingId === selected.id}
              inDraft={draftActionIds.has(selected.id)}
              previewing={previewPendingId === selected.id}
              onAddToDraft={() => onAddToDraft(selected)}
              onDetect={() => onDetect(selected.id)}
              onPreview={(parameterOverride) => onPreview(selected, parameterOverride)}
              onError={onError}
            />
          )}
        </section>
      </div>

      {/* 追加パネルは項目一覧の「下」に置く。
          上に積むと、そのカテゴリを開いた人が探しに来た項目が1つも見えない。
          実測で、見た目カテゴリはパネルだけで画面の 92% を占めていた。 */}
      {selectedCategory === "appearance" ? (
        <AppearanceScenesPanel
          actions={actions}
          bootstrap={bootstrap}
          dataMode={dataMode}
          onPreview={onPreviewScene}
          previewPendingKey={previewPendingId}
        />
      ) : null}
      {selectedCategory === "appearance" ? <ThemeSchedulePanel dataMode={dataMode} /> : null}
      {selectedCategory === "appearance" ? <TaskbarAutoHidePanel dataMode={dataMode} /> : null}
      {selectedCategory === "storage" ? <StorageHistoryPanel dataMode={dataMode} /> : null}
      {selectedCategory === "storage" ? <TempCleanupPanel dataMode={dataMode} /> : null}

    </div>
  );
}

interface ActionDetailProps {
  action: ActionPresentation;
  bootstrap: BootstrapStatus | null;
  dataMode: DataMode;
  detecting: boolean;
  inDraft: boolean;
  previewing: boolean;
  onAddToDraft: () => void;
  onDetect: () => void;
  onPreview: (parameterOverride?: Record<string, string>) => void;
  onError: (error: unknown) => void;
}

function ActionDetail({ action, bootstrap, dataMode, detecting, inDraft, previewing, onAddToDraft, onDetect, onPreview, onError }: ActionDetailProps) {
  const [detailsOpen, setDetailsOpen] = useState(false);
  // 電源モードだけは、押す前にどれにするかを選ばせる。
  // 既定を1つ決め打ちにすると「選ぶ」と書いてあるのに選べない画面になる。
  const [powerMode, setPowerMode] = useState("balanced");
  const picksPowerMode = action.id === "power.mode_switch";
  const picksDefaultPrinter = action.id === "session.default_printer";
  const [printerScene, setPrinterScene] = useState("");
  const [selectedPrinter, setSelectedPrinter] = useState("");
  const current = action.currentState;
  const readOnly = action.availability === "read_only" || action.availability === "detect_only";
  const needsBinding = action.id === "games.process_watch";
  const mutationAllowed = isMutationAllowed(dataMode, bootstrap, action);
  const reason = blockReason(dataMode, bootstrap, action);
  const profileEligible = action.autoApplyEligible === true
    && (action.kind === "persistent" || action.kind === "session");
  const observationLike = action.kind === "observation" || action.kind === "guided";
  const guidedCandidate = action.kind === "guided" && action.methodClass === "unverified_storage";
  const printerOptions = picksDefaultPrinter && current?.kind === "known"
    ? (current.items ?? []).slice(1)
    : [];
  const printerSelectionReady = !picksDefaultPrinter
    || (printerScene.trim().length > 0 && selectedPrinter.length > 0 && printerOptions.includes(selectedPrinter));

  return (
    <div className="action-detail__inner">
      {/* いちばん大事なのは「何ができるか」。以前はここに危険度チップと内部ID(session.prevent_sleep)と
          v1 が先に来て、名前が3番目だった。内部IDとバージョンは詳細の中へ移した。 */}
      <div className="detail-title-row">
        <div>
          <h2>{action.name}</h2>
          <p>{screenText(action.description, "Windowsのこの項目について、現在の状態を確認してから安全な範囲だけを扱います。")}</p>
        </div>
      </div>
      <p className="audience-line"><Icon name="info" size={16} />{screenText(action.audience, "この項目の状態を確認してから変更したい人向け")}</p>
      {!picksPowerMode ? null : (
        <div aria-label="どのモードにするか" className="mode-choice">
          <span>どれにしますか</span>
          <div className="segmented" role="tablist">
            {[
              { value: "best_efficiency", label: "電池優先" },
              { value: "balanced", label: "バランス" },
              { value: "best_performance", label: "パフォーマンス優先" },
            ].map((option) => (
              <button
                aria-selected={powerMode === option.value}
                className="segmented__item"
                key={option.value}
                onClick={() => setPowerMode(option.value)}
                role="tab"
                type="button"
              >
                {option.label}
              </button>
            ))}
          </div>
          {/* この PC が受け付けない値がある。押す前には分からないので、そう書いておく。 */}
          <small>選べない値があるPCもあります。その場合は何も変更せずにお知らせします。</small>
        </div>
      )}
      {!picksDefaultPrinter ? null : (
        <div aria-label="場面と既定プリンターを選ぶ" className="printer-choice">
          <label>
            <span>場面の名前</span>
            <input
              autoComplete="off"
              maxLength={64}
              onChange={(event) => setPrinterScene(event.target.value)}
              placeholder="例: 自宅、職場、ラベル印刷"
              type="text"
              value={printerScene}
            />
          </label>
          <label>
            <span>今回だけ既定にするプリンター</span>
            <select
              disabled={printerOptions.length === 0}
              onChange={(event) => setSelectedPrinter(event.target.value)}
              value={selectedPrinter}
            >
              <option value="">選択してください</option>
              {printerOptions.map((printer) => <option key={printer} value={printer}>{printer}</option>)}
            </select>
          </label>
          {current?.kind === "policy_managed" ? (
            <small>Windowsの「通常使うプリンターをWindowsで管理する」が有効なため、変更しません。</small>
          ) : printerOptions.length === 0 ? (
            <small>別のインストール済みプリンターがありません。状態を再確認してください。</small>
          ) : (
            <small>場所は自動推測しません。プリンターの追加や印刷も行いません。</small>
          )}
          <button className="secondary-button" disabled={dataMode !== "live" || detecting} onClick={onDetect} type="button">
            {detecting ? <Icon className="spin" name="spinner" /> : <Icon name="search" />}候補を再確認
          </button>
        </div>
      )}
      <div aria-label="現在と適用後の状態" className="state-comparison">
        <div className={`state-panel state-panel--${current?.kind ?? "unknown"}`}>
          <span>現在</span>
          {detecting ? <strong className="state-loading"><Icon className="spin" name="spinner" />確認中</strong> : <strong>{current == null ? "未検出" : screenText(current.label, "Windowsから読み取った状態")}</strong>}
          <small>{current?.detail === undefined
            ? (dataMode === "catalog" ? "安全コア未接続のため現在値は表示していません。" : "状態を確認してください。")
            : screenText(current.detail, "Windowsから読み取った現在の状態です。")}</small>
          {current?.items === undefined || current.items.length === 0 ? null : (
            <div aria-label="検出した項目" className="state-item-list">
              <span>検出した項目</span>
              <ul>
                {current.items.map((item, index) => <li key={`${item}-${index}`}>{screenText(item, "Windowsから読み取った項目です。")}</li>)}
              </ul>
            </div>
          )}
        </div>
        <span className="state-arrow"><Icon name="arrow" /></span>
        <div className="state-panel state-panel--desired">
          <span>{guidedCandidate ? "設計状態" : action.kind === "guided" ? "案内先" : action.kind === "observation" ? "確認する内容" : "適用後"}</span>
          <strong>{screenText(action.desiredState, "この項目に必要な範囲だけを扱う")}</strong>
        </div>
      </div>
      {/* 常に5個並べると2行を占め、どれも同じ重みで読まれない。
          既定では「戻せるか」と、注意が要る場合だけを出す。残りは詳細の中にある。 */}
      <div aria-label="この項目の性質" className="attribute-chips">
        <span className={`attribute-chip attribute-chip--${action.riskLevel === "safe" ? "safe" : action.riskLevel}`}>
          <Icon name={action.reversible ? "check" : "warning"} size={14} />
          {action.reversible ? "元に戻せます" : "元に戻せません"}
        </span>
        {action.requiresAdmin ? <span className="attribute-chip attribute-chip--caution">管理者の確認が出ます</span> : null}
        {action.requiresRestart ? <span className="attribute-chip attribute-chip--caution">Windowsの再起動が必要</span> : null}
        {action.requiresExplorerRestart ? <span className="attribute-chip attribute-chip--caution">反映にエクスプローラーの再起動が必要</span> : null}
        {action.riskLevel === "experimental" ? <span className="attribute-chip attribute-chip--experimental">実験的</span> : null}
      </div>
      {detailsOpen ? (
        <div className="detail-disclosure" id={`details-${action.id}`}>
          <div><span className="detail-disclosure__label">{observationLike ? "確認方法" : "変更方法"}</span><p>{methodSummaryForScreen(action)}</p></div>
          <ul>{detailPointsForScreen(action).map((point) => <li key={point}>{point}</li>)}</ul>
          <dl className="compatibility-grid">
            <div><dt>方式</dt><dd>{action.kind === "persistent" ? "永続設定" : action.kind === "session" ? "セッション" : guidedCandidate ? "設計候補（変更不可）" : action.kind === "guided" ? "Windows設定案内（PCカスタム変更なし）" : "観測"}</dd></div>
            <div><dt>根拠分類</dt><dd>{methodClassLabel(action.methodClass)}</dd></div>
            <div><dt>危険度</dt><dd>{riskLabel(action.riskLevel)}</dd></div>
            <div><dt>管理者権限</dt><dd>{action.requiresAdmin ? "必要" : "不要"}</dd></div>
            <div><dt>再起動</dt><dd>{action.requiresRestart ? "Windowsの再起動" : action.requiresExplorerRestart ? "エクスプローラーの再起動" : "不要"}</dd></div>
            <div><dt>Windows Updateの影響</dt><dd>{updateImpactLabel(action.updateImpact)}</dd></div>
          </dl>
        </div>
      ) : null}
      {reason.length > 0 && !readOnly ? <p className="blocked-reason"><Icon name="info" size={15} />{reason}</p> : null}
      <div className="detail-actions">
        <button aria-controls={`details-${action.id}`} aria-expanded={detailsOpen} className="secondary-button" onClick={() => setDetailsOpen((open) => !open)} type="button"><Icon name="info" />{detailsOpen ? "詳細を閉じる" : "詳細を見る"}</button>
        {readOnly ? (
          <button className="primary-button" disabled={dataMode !== "live" || detecting || needsBinding} onClick={onDetect} type="button">{detecting ? <Icon className="spin" name="spinner" /> : <Icon name={needsBinding ? "info" : "search"} />}{needsBinding ? "実行ファイル登録後に確認" : "状態を確認"}</button>
        ) : (
          <button
            className="primary-button"
            disabled={!mutationAllowed || previewing || !printerSelectionReady}
            onClick={() => onPreview(
              picksPowerMode
                ? { mode: powerMode }
                : picksDefaultPrinter
                  ? { scene: printerScene.trim(), printer: selectedPrinter }
                  : undefined,
            )}
            type="button"
          >
            {previewing ? <Icon className="spin" name="spinner" /> : <Icon name="arrow" />}
            {previewing ? "プレビュー作成中" : "適用プレビュー"}
          </button>
        )}
        {/* 「自動適用の対象外」は永久に押せないボタンだった。押せないものはボタンではなく状態なので、
            文で書く。押せる可能性があるときだけボタンを出す。 */}
        {profileEligible ? (
          <button className="secondary-button" disabled={inDraft} onClick={onAddToDraft} type="button">
            <Icon name={inDraft ? "check" : "plus"} />{inDraft ? "下書きに追加済み" : "モードへ追加"}
          </button>
        ) : (
          <span className="detail-note">この項目はモードの自動適用には入れられません</span>
        )}
        {action.settingsPage ? (
          <button
            className="secondary-button"
            disabled={dataMode !== "live"}
            onClick={() => void openWindowsSettings(action.id).catch(onError)}
            type="button"
          >
            <Icon name="arrow" />Windowsの設定を開く
          </button>
        ) : null}
      </div>
      {action.requiresExplorerRestart ? <ExplorerRestartPanel dataMode={dataMode} /> : null}
      {action.settingsPage && action.availability !== "mutable" ? (
        <p className="blocked-reason"><Icon name="info" size={15} />この項目はWindowsの設定画面から変更できます。PCカスタムは設定画面を案内し、OS設定は変更しません。</p>
      ) : null}
    </div>
  );
}
