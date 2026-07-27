import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  commitPreview,
  createProfile,
  deleteProfile,
  detectAction,
  listProfiles,
  loadCoreSnapshot,
  previewActions,
  publicErrorCode,
  publicErrorMessage,
  reconcileNow,
  restoreProfileNow,
  rollbackItem,
  runProfileNow,
  setProfileEnabled,
} from "./backend";
import { STATIC_ACTIONS } from "./catalog";
import { ActionBrowser } from "./components/ActionBrowser";
import { CommandPalette, type PaletteCommand } from "./components/CommandPalette";
import { Dialog } from "./components/Dialog";
import { HomeView } from "./components/HomeView";
import { Icon } from "./components/Icon";
import { ProfilesView } from "./components/ProfilesView";
import { SetupView } from "./components/SetupView";
import { Sidebar } from "./components/Sidebar";
import { TimelineView } from "./components/TimelineView";
import type {
  ActionPresentation,
  BootstrapStatus,
  CategoryId,
  CreateProfileRequest,
  DataMode,
  JsonValue,
  PreviewResponse,
  ProfileDraftItem,
  StoredProfile,
  TimelineItem,
  ViewId,
  CommitItem,
} from "./model";

interface UiError {
  message: string;
  code: string;
}

const NOTICE_DETAIL_LIMIT = 3;
const NOTICE_DETAIL_CHARACTER_LIMIT = 80;

function shortenNoticeDetail(detail: string): string {
  const characters = Array.from(detail);
  if (characters.length <= NOTICE_DETAIL_CHARACTER_LIMIT) return detail;
  return `${characters.slice(0, NOTICE_DETAIL_CHARACTER_LIMIT).join("")}…`;
}

function commitNotice(message: string, details: readonly string[] | undefined): string {
  if (details === undefined || details.length === 0) return message;
  const shown = details
    .slice(0, NOTICE_DETAIL_LIMIT)
    .map(shortenNoticeDetail)
    .join(" ／ ");
  const remainder =
    details.length > NOTICE_DETAIL_LIMIT
      ? ` ／ ほか${details.length - NOTICE_DETAIL_LIMIT}件`
      : "";
  return `${message} ${shown}${remainder}`;
}

function parametersForAction(actionId: string): Record<string, JsonValue> {
  if (actionId === "session.prevent_sleep") return { keepDisplayOn: false };
  if (actionId === "explorer.show_extensions") return { show: true };
  if (actionId === "explorer.show_hidden") return { show: true };
  if (actionId === "explorer.clock_seconds") return { show: true };
  if (actionId === "appearance.transparency") return { enabled: true };
  if (actionId === "taskbar.task_view") return { show: true };
  if (actionId === "taskbar.widgets") return { show: true };
  if (actionId === "explorer.item_checkboxes") return { show: true };
  if (actionId === "explorer.compact_view") return { enabled: true };
  if (actionId === "theme.color_mode") return { mode: "dark" };
  if (actionId === "power.active_scheme_check") return {};
  if (actionId === "power.active_scheme_switch") return { scheme: "balanced" };
  if (actionId === "games.process_watch") return {};
  if (actionId === "games.readiness_check") return {};
  if (actionId === "taskbar.search_mode") return { mode: "search_box" };
  if (actionId === "taskbar.alignment") return { alignment: "left" };
  if (actionId === "start.layout") return { layout: "more_pins" };
  if (actionId === "start.recommendations") return { enabled: false };
  if (actionId === "explorer.launch_target") return { target: "this_pc" };
  if (actionId === "explorer.recent_files") return { show: true };
  if (actionId === "taskbar.button_grouping") return { mode: "when_full" };
  if (actionId === "taskbar.flashing") return { enabled: true };
  if (actionId === "taskbar.share_window") return { enabled: true };
  if (actionId === "taskbar.show_desktop") return { enabled: true };
  if (actionId === "search.recent_on_hover") return { enabled: false };
  if (actionId === "taskbar.multi_monitor") return { enabled: true };
  if (actionId === "taskbar.multi_monitor_mode") return { mode: "window_monitor" };
  if (actionId === "taskbar.secondary_button_grouping") return { mode: "when_full" };
  if (actionId === "start.show_all_pins") return { enabled: true };
  if (actionId === "start.recent_apps") return { show: true };
  if (actionId === "appearance.accent_start_taskbar") return { enabled: true };
  if (actionId === "appearance.accent_title_bars") return { enabled: true };
  if (actionId === "appearance.auto_accent") return { enabled: true };
  if (actionId === "games.game_mode") return { enabled: true };
  if (actionId === "games.controller_game_bar") return { enabled: false };
  if (actionId === "devices.autoplay") return { enabled: true };
  if (actionId === "notifications.usb_errors") return { enabled: true };
  if (actionId === "notifications.weak_charger") return { enabled: true };
  if (actionId === "input.autocorrect") return { enabled: true };
  if (actionId === "input.double_space_period") return { enabled: true };
  if (actionId === "input.auto_shift") return { enabled: true };
  if (actionId === "input.voice_typing_key") return { enabled: true };
  if (actionId === "input.multilingual_suggestions") return { enabled: true };
  if (actionId === "explorer.status_bar") return { show: true };
  if (actionId === "explorer.info_tips") return { show: true };
  if (actionId === "explorer.hide_empty_drives") return { hide: true };
  if (actionId === "explorer.nav_expand_current") return { enabled: true };
  if (actionId === "explorer.nav_show_all") return { enabled: true };
  if (actionId === "explorer.separate_process") return { enabled: true };
  if (actionId === "explorer.icons_only") return { enabled: false };
  if (actionId === "explorer.drive_letters") return { show: true };
  if (actionId === "explorer.preview_handlers") return { enabled: true };
  if (actionId === "explorer.sharing_wizard") return { enabled: true };
  if (actionId === "explorer.always_show_menus") return { enabled: true };
  if (actionId === "appearance.taskbar_animations") return { enabled: true };
  if (actionId === "notifications.toast_banners") return { enabled: true };
  if (actionId === "setup.startup_inventory") return {};
  if (actionId === "storage.free_space_check") return {};
  if (actionId === "storage.temp_files_check") return {};
  if (actionId === "appearance.accent_color_check") return {};
  if (actionId === "appearance.window_color") return { color: "teal" };
  if (actionId === "setup.powertoys_status") return {};
  if (actionId === "setup.launch_apps") return { bundle: "study" };
  if (actionId === "setup.windows_update_status") return {};
  if (actionId === "setup.default_apps") return {};
  if (actionId === "setup.window_layout") return {};
  if (actionId === "setup.audio_output") return {};
  return {};
}

export function App() {
  const [view, setView] = useState<ViewId>("home");
  const [dataMode, setDataMode] = useState<DataMode>("loading");
  const [bootstrap, setBootstrap] = useState<BootstrapStatus | null>(null);
  const [actions, setActions] = useState<readonly ActionPresentation[]>(STATIC_ACTIONS);
  const [timeline, setTimeline] = useState<readonly TimelineItem[]>([]);
  const [selectedCategory, setSelectedCategory] = useState<CategoryId>("session");
  const [selectedActionId, setSelectedActionId] = useState<string | null>(STATIC_ACTIONS[0]?.id ?? null);
  const [detectionPendingId, setDetectionPendingId] = useState<string | null>(null);
  const [previewPendingId, setPreviewPendingId] = useState<string | null>(null);
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  const [previewConfirmed, setPreviewConfirmed] = useState(false);
  const [commitPending, setCommitPending] = useState(false);
  const [rollbackTarget, setRollbackTarget] = useState<TimelineItem | null>(null);
  const [rollbackPendingId, setRollbackPendingId] = useState<string | null>(null);
  const [recoveryBusy, setRecoveryBusy] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [draftOpen, setDraftOpen] = useState(false);
  const [profileDraft, setProfileDraft] = useState<readonly ProfileDraftItem[]>([]);
  const [profiles, setProfiles] = useState<readonly StoredProfile[]>([]);
  const [profileBusy, setProfileBusy] = useState(false);
  const [uiError, setUiError] = useState<UiError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // 直前に適用した項目。その場で戻せるようにするために持つ。
  // 以前は適用後にタイムラインへ強制移動しており、戻すのに画面移動と項目探しが必要だった。
  const [justApplied, setJustApplied] = useState<CommitItem[]>([]);
  const [undoPending, setUndoPending] = useState(false);
  const initialLoadStarted = useRef(false);

  const handleUiError = useCallback((error: unknown) => {
    setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
  }, []);

  const refreshSnapshot = useCallback(async (showLoading: boolean) => {
    if (showLoading) setDataMode("loading");
    try {
      const snapshot = await loadCoreSnapshot();
      setBootstrap(snapshot.bootstrap);
      setActions(snapshot.actions);
      setTimeline(snapshot.timeline);
      setDataMode("live");
    } catch (error: unknown) {
      setActions(STATIC_ACTIONS);
      setTimeline([]);
      setBootstrap({
        mode: "read_only",
        osLabel: "安全コア未接続",
        build: null,
        message: "静的カタログのみ表示しています。OSへの変更操作は停止しています。",
        recoveryCount: 0,
      });
      setDataMode("catalog");
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    }
  }, []);

  const refreshProfiles = useCallback(async () => {
    try {
      setProfiles(await listProfiles());
    } catch {
      // 閲覧モード（安全コア未接続）ではプロファイルを空表示にする。
      setProfiles([]);
    }
  }, []);

  useEffect(() => {
    if (initialLoadStarted.current) return;
    initialLoadStarted.current = true;
    void refreshSnapshot(true);
    void refreshProfiles();
  }, [refreshProfiles, refreshSnapshot]);

  useEffect(() => {
    function handleShortcut(event: globalThis.KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLocaleLowerCase("en-US") === "k") {
        event.preventDefault();
        setCommandOpen(true);
      }
    }
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, []);

  const handleDetect = useCallback(async (actionId: string) => {
    if (dataMode !== "live") return;
    if (actionId === "games.process_watch") return;
    setDetectionPendingId(actionId);
    setUiError(null);
    try {
      const response = await detectAction(actionId);
      setActions((current) => current.map((action) => action.id === response.actionId ? { ...action, currentState: response.state } : action));
    } catch (error: unknown) {
      const message = publicErrorMessage(error);
      setActions((current) => current.map((action) => action.id === actionId ? { ...action, currentState: { kind: "error", label: "確認できません", detail: message } } : action));
      setUiError({ message, code: publicErrorCode(error) });
    } finally {
      setDetectionPendingId((current) => current === actionId ? null : current);
    }
  }, [dataMode]);

  const openAction = useCallback((action: ActionPresentation) => {
    setSelectedCategory(action.category);
    setSelectedActionId(action.id);
    setView("actions");
    void handleDetect(action.id);
  }, [handleDetect]);

  const openCategory = useCallback((category: CategoryId) => {
    const first = actions.find((action) => action.category === category);
    setSelectedCategory(category);
    setSelectedActionId(first?.id ?? null);
    setView("actions");
    if (first !== undefined) void handleDetect(first.id);
  }, [actions, handleDetect]);

  const navigate = useCallback((target: ViewId) => {
    setView(target);
    if (target === "actions" && selectedActionId !== null) void handleDetect(selectedActionId);
  }, [handleDetect, selectedActionId]);

  const paletteCommands = useMemo<readonly PaletteCommand[]>(() => {
    const navigation: PaletteCommand[] = [
      { id: "nav-home", label: "ホームを開く", description: "結果タイルへ移動", icon: "home", execute: () => setView("home") },
      { id: "nav-actions", label: "Actionを探す", description: "カテゴリと詳細を表示", icon: "action", execute: () => navigate("actions") },
      { id: "nav-profiles", label: "モード", description: "ゲーム・勉強・作業の準備を管理", icon: "game", execute: () => setView("profiles") },
      { id: "nav-timeline", label: "変更を元へ戻す", description: "タイムラインへ移動", icon: "timeline", execute: () => setView("timeline") },
    ];
    const actionCommands: PaletteCommand[] = actions.map((action): PaletteCommand => ({
      id: `action-${action.id}`,
      label: action.name,
      description: action.description,
      icon: action.category === "games" ? "game" : action.category === "appearance" ? "appearance" : action.category === "explorer" ? "explorer" : action.category === "power" ? "power" : "focus",
      execute: () => openAction(action),
    }));
    return navigation.concat(actionCommands);
  }, [actions, navigate, openAction]);

  async function requestActionPreview(actionId: string, parameters: Record<string, JsonValue>) {
    if (dataMode !== "live") return;
    setPreviewPendingId(actionId);
    setUiError(null);
    try {
      const result = await previewActions({ actions: [{ actionId, parameters }] });
      setPreview(result);
      setPreviewConfirmed(false);
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setPreviewPendingId(null);
    }
  }

  function requestPreview(action: ActionPresentation) {
    return requestActionPreview(action.id, parametersForAction(action.id));
  }

  function requestPowerToysLaunch() {
    return requestActionPreview("setup.launch_apps", { bundle: "power_toys" });
  }

  async function confirmPreview() {
    if (preview === null || !previewConfirmed) return;
    setCommitPending(true);
    setUiError(null);
    try {
      const result = await commitPreview({ previewToken: preview.previewToken });
      if (result.status === "succeeded") {
        setNotice(commitNotice(result.message, result.details));
        // 戻す手段をその場に置く。画面は動かさない。
        setJustApplied(result.items ?? []);
      } else {
        setUiError({ message: result.message, code: result.status.toLocaleUpperCase("en-US") });
        setJustApplied([]);
      }
      setPreview(null);
      setPreviewConfirmed(false);
      await refreshSnapshot(false);
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setCommitPending(false);
    }
  }

  /// 直前に適用した分を、その場でまとめて元へ戻す。
  /// 逆順に戻すのは、適用が順序を持つため（後に適用したものから解く）。
  async function undoJustApplied() {
    if (justApplied.length === 0) return;
    setUndoPending(true);
    setUiError(null);
    try {
      for (const item of [...justApplied].reverse()) {
        const result = await rollbackItem({ itemId: item.itemId });
        if (result.status === "recovery_required") {
          setUiError({ message: result.message, code: "RECOVERY_REQUIRED" });
          return;
        }
      }
      setNotice("元へ戻しました。");
      setJustApplied([]);
      await refreshSnapshot(false);
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setUndoPending(false);
    }
  }

  async function confirmRollback() {
    if (rollbackTarget === null) return;
    const itemId = rollbackTarget.itemId;
    setRollbackPendingId(itemId);
    setUiError(null);
    try {
      const result = await rollbackItem({ itemId });
      if (result.status === "recovery_required") {
        setUiError({ message: result.message, code: "RECOVERY_REQUIRED" });
      } else {
        setNotice(result.message);
      }
      setRollbackTarget(null);
      await refreshSnapshot(false);
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setRollbackPendingId(null);
    }
  }

  async function runReconcile() {
    if (dataMode !== "live") return;
    setRecoveryBusy(true);
    setUiError(null);
    try {
      const result = await reconcileNow();
      if (result.status === "recovery_required") {
        setUiError({ message: result.message, code: "RECOVERY_REQUIRED" });
      } else {
        setNotice(result.message);
      }
      await refreshSnapshot(false);
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setRecoveryBusy(false);
    }
  }

  async function handleCreateProfile(request: CreateProfileRequest) {
    setProfileBusy(true);
    setUiError(null);
    try {
      const created = await createProfile(request);
      setNotice(created.executablePath === undefined ? `手動モード「${created.name}」を作成しました。「いま実行」を押すまで適用しません。` : `ゲームプロファイル「${created.name}」を作成しました。自動適用はまだオフです。`);
      await refreshProfiles();
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setProfileBusy(false);
    }
  }

  async function handleSetProfileEnabled(id: string, enabled: boolean) {
    setProfileBusy(true);
    setUiError(null);
    try {
      await setProfileEnabled(id, enabled);
      setNotice(enabled ? "自動適用を有効にしました。" : "自動適用を止めました。");
      await refreshProfiles();
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setProfileBusy(false);
    }
  }

  async function handleRunProfile(id: string) {
    setProfileBusy(true);
    setUiError(null);
    try {
      const result = await runProfileNow(id);
      setNotice(result.message);
      await Promise.all([refreshProfiles(), refreshSnapshot(false)]);
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setProfileBusy(false);
    }
  }

  async function handleRestoreProfile(id: string) {
    setProfileBusy(true);
    setUiError(null);
    try {
      const result = await restoreProfileNow(id);
      setNotice(result.message);
      await Promise.all([refreshProfiles(), refreshSnapshot(false)]);
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setProfileBusy(false);
    }
  }
  async function handleDeleteProfile(id: string) {
    setProfileBusy(true);
    setUiError(null);
    try {
      await deleteProfile(id);
      setNotice("プロファイルを削除しました。");
      await refreshProfiles();
    } catch (error: unknown) {
      setUiError({ message: publicErrorMessage(error), code: publicErrorCode(error) });
    } finally {
      setProfileBusy(false);
    }
  }

  function addToDraft(action: ActionPresentation) {
    if (action.autoApplyEligible !== true) {
      setNotice("このActionは明示操作専用のため、モードへ追加できません。");
      return;
    }
    setProfileDraft((current) => current.some((item) => item.actionId === action.id) ? current : [...current, { actionId: action.id, title: action.name }]);
    setNotice("プロファイル下書きへ追加しました。保存や自動適用はまだ行っていません。");
  }

  const draftIds = useMemo(() => new Set(profileDraft.map((item) => item.actionId)), [profileDraft]);
  const powerToysAction = actions.find((action) => action.id === "setup.powertoys_status");

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">本文へ移動</a>
      <Sidebar activeView={view} dataMode={dataMode} onNavigate={navigate} onOpenDraft={() => setDraftOpen(true)} profileCount={profileDraft.length} />
      <div className="app-surface">
        <header className="topbar">
          <span className="topbar-context">標準ユーザー</span>
          <button aria-label="コマンドパレットを開く" className="command-trigger" onClick={() => setCommandOpen(true)} type="button"><Icon name="search" /><span>結果を検索</span><kbd>Ctrl K</kbd></button>
        </header>
        {uiError === null ? null : (
          <div className="error-banner" role="alert"><Icon name="warning" /><div><strong>{dataMode === "catalog" ? "閲覧モードで開いています" : "処理を完了できませんでした"}</strong><span>{uiError.message}</span>
            {/* エラーコードは記録と問い合わせのためのもので、利用者が読む情報ではない。
                いきなり見せず、必要な人だけ開けるようにする。 */}
            <details className="error-banner__code"><summary>問い合わせ用の情報</summary><code>{uiError.code}</code></details></div><button aria-label="エラーを閉じる" onClick={() => setUiError(null)} type="button"><Icon name="close" /></button></div>
        )}
        <main aria-busy={dataMode === "loading"} id="main-content">
          {view === "home" ? (
            <HomeView actions={actions} bootstrap={bootstrap} dataMode={dataMode} onOpenCategory={openCategory} onOpenTimeline={() => setView("timeline")} onOpenView={(target) => setView(target)} onReconcile={() => void runReconcile()} recoveryBusy={recoveryBusy} timeline={timeline} />
          ) : view === "actions" ? (
            <ActionBrowser actions={actions} bootstrap={bootstrap} dataMode={dataMode} detectionPendingId={detectionPendingId} draftActionIds={draftIds} onAddToDraft={addToDraft} onDetect={(id) => void handleDetect(id)} onError={handleUiError} onPreview={(action) => void requestPreview(action)} onSelectAction={(id) => { const action = actions.find((candidate) => candidate.id === id); if (action !== undefined) openAction(action); }} onSelectCategory={openCategory} previewPendingId={previewPendingId} selectedActionId={selectedActionId} selectedCategory={selectedCategory} />
          ) : view === "profiles" ? (
            <ProfilesView actions={actions} busy={profileBusy} dataMode={dataMode} onChanged={() => void refreshProfiles()} onCreate={(request) => void handleCreateProfile(request)} onDelete={(id) => void handleDeleteProfile(id)} onOpenActions={() => navigate("actions")} onParametersForAction={parametersForAction} onRestore={(id) => void handleRestoreProfile(id)} onRun={(id) => void handleRunProfile(id)} onSetEnabled={(id, enabled) => void handleSetProfileEnabled(id, enabled)} profiles={profiles} />
          ) : view === "setup" ? (
            <SetupView
              actions={actions}
              bootstrap={bootstrap}
              dataMode={dataMode}
              detectionPendingId={detectionPendingId}
              onDetect={(id) => void handleDetect(id)}
              onError={handleUiError}
              onNotice={setNotice}
              onPowerToysDetect={() => void handleDetect("setup.powertoys_status")}
              onPowerToysLaunch={() => void requestPowerToysLaunch()}
              onPreview={(action) => void requestPreview(action)}
              powerToysAction={powerToysAction}
              powerToysDetecting={detectionPendingId === "setup.powertoys_status"}
              powerToysLaunching={previewPendingId === "setup.launch_apps"}
              previewPendingId={previewPendingId}
            />
          ) : (
            <TimelineView bootstrap={bootstrap} dataMode={dataMode} items={timeline} onOpenActions={() => navigate("actions")} onRequestRollback={setRollbackTarget} onRetryRecovery={() => void runReconcile()} recoveryBusy={recoveryBusy} rollbackPendingId={rollbackPendingId} />
          )}
        </main>
      </div>

      {commandOpen ? <CommandPalette commands={paletteCommands} onClose={() => setCommandOpen(false)} /> : null}
      {preview === null ? null : (
        <Dialog
          description="現在値を再確認した結果です。内容が変わっていた場合、確認済みpreviewはcommit時に拒否されます。"
          footer={<><button className="secondary-button" disabled={commitPending} onClick={() => setPreview(null)} type="button">キャンセル</button><button className="primary-button" disabled={!previewConfirmed || commitPending} onClick={() => void confirmPreview()} type="button">{commitPending ? <Icon className="spin" name="spinner" /> : <Icon name="check" />}確認して適用</button></>}
          onClose={() => { if (!commitPending) setPreview(null); }}
          title="適用プレビュー"
          width="wide"
        >
          <div className="preview-list">
            {preview.changes.map((change) => <article className="preview-change" key={change.actionId}><header><span className={`risk-label risk-label--${change.riskLevel}`}>{change.riskLevel === "safe" ? "低リスク" : change.riskLevel === "caution" ? "注意" : "実験的"}</span><h3>{change.title}</h3></header><div><span><small>現在</small><strong>{change.before}</strong></span><Icon name="arrow" /><span><small>適用後</small><strong>{change.after}</strong></span></div><p>{change.method}</p><small>対象: {change.resourceLabel} ／ {change.reversible ? "元に戻せます" : "変更はありません"}</small></article>)}
          </div>
          {preview.warnings.length === 0 ? null : <div className="preview-warnings"><strong><Icon name="warning" />確認事項</strong><ul>{preview.warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul></div>}
          <label className="confirmation-check"><input checked={previewConfirmed} onChange={(event) => setPreviewConfirmed(event.target.checked)} type="checkbox" /><span>変更前の状態が保存され、失敗時は逆順で復元されることを確認しました。</span></label>
        </Dialog>
      )}
      {rollbackTarget === null ? null : (
        <Dialog
          description="現在の状態がPCカスタムの適用値と一致する場合だけ、保存済みの変更前状態へ戻します。"
          footer={<><button className="secondary-button" disabled={rollbackPendingId !== null} onClick={() => setRollbackTarget(null)} type="button">キャンセル</button><button className="danger-button" disabled={rollbackPendingId !== null} onClick={() => void confirmRollback()} type="button">{rollbackPendingId !== null ? <Icon className="spin" name="spinner" /> : <Icon name="undo" />}この変更だけ戻す</button></>}
          onClose={() => { if (rollbackPendingId === null) setRollbackTarget(null); }}
          title="変更を元へ戻しますか"
        >
          <div className="rollback-summary"><strong>{rollbackTarget.title}</strong><div><span><small>現在</small>{rollbackTarget.after}</span><Icon name="arrow" /><span><small>復元後</small>{rollbackTarget.before}</span></div><p>外部変更を検出した場合は上書きせず、復旧が必要な項目として停止します。</p></div>
        </Dialog>
      )}
      {draftOpen ? (
        <Dialog
          description="この下書きは現在の画面内だけに保持されます。OS変更や自動適用は行いません。"
          footer={<button className="primary-button" onClick={() => setDraftOpen(false)} type="button">閉じる</button>}
          onClose={() => setDraftOpen(false)}
          title="プロファイル下書き"
        >
          {profileDraft.length === 0 ? <div className="dialog-empty"><Icon name="plus" /><strong>Actionはまだありません</strong><span>Action詳細から「プロファイルへ追加」を選んでください。</span></div> : <ul className="draft-list">{profileDraft.map((item) => <li key={item.actionId}><span><strong>{item.title}</strong><code>{item.actionId}</code></span><button aria-label={`${item.title}を下書きから削除`} onClick={() => setProfileDraft((current) => current.filter((candidate) => candidate.actionId !== item.actionId))} type="button"><Icon name="close" /></button></li>)}</ul>}
        </Dialog>
      ) : null}
      {justApplied.length === 0 ? null : (
        <div aria-live="polite" className="undo-bar">
          <Icon name="check" size={16} />
          <span>
            適用しました
            <strong>{justApplied.map((item) => item.name).join("、")}</strong>
          </span>
          <button className="secondary-button" disabled={undoPending} onClick={() => void undoJustApplied()} type="button">
            {undoPending ? <Icon className="spin" name="spinner" /> : <Icon name="undo" />}元に戻す
          </button>
          <button aria-label="この案内を閉じる" className="icon-button" onClick={() => setJustApplied([])} type="button">
            <Icon name="close" size={15} />
          </button>
        </div>
      )}
      {notice === null ? null : <div aria-live="polite" className="notice-toast"><Icon name="check" /><span>{notice}</span><button aria-label="通知を閉じる" onClick={() => setNotice(null)} type="button"><Icon name="close" size={15} /></button></div>}
    </div>
  );
}
