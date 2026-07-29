import { invoke } from "@tauri-apps/api/core";

import type {
  ActionPresentation,
  BootstrapStatus,
  CategoryId,
  CommitPreviewRequest,
  CommitResult,
  CoreErrorShape,
  CreateProfileRequest,
  DetectionResponse,
  HealthReport,
  ImportPreviewItem,
  ImportResult,
  InstallOutcome,
  ManualProfileResult,
  ModeRibbonColor,
  OffscreenWindowScan,
  OffscreenWindowUndo,
  SetupAppDto,
  TempCleanupOutcome,
  TempCleanupPlan,
  ThemeSchedule,
  ThemeScheduleState,
  PreviewActionsRequest,
  PreviewResponse,
  ReconcileResult,
  RollbackItemRequest,
  StoredProfile,
  TaskbarAutoHideState,
  TimelineItem,
  WindowLayoutStatus,
  ShellRestartOutcome,
} from "./model";

export interface CoreSnapshot {
  bootstrap: BootstrapStatus;
  actions: readonly ActionPresentation[];
  timeline: readonly TimelineItem[];
}

type RawActionPresentation = Omit<ActionPresentation, "category"> & { category: string };

/**
 * 画面が扱えるカテゴリ。
 *
 * ここには**一覧を持たない。** 以前は Action ID ごとのカテゴリ表を持っていて、
 * 新しい Action を足すたびに書き足す必要があり、忘れると `list_actions` が
 * まるごと失敗して**アプリが何も表示しなくなる。** 実際に2つ書き忘れた。
 * 表はコアが既に持っているので、こちらは受け取った値が既知かどうかだけを見る。
 */
const KNOWN_CATEGORIES: ReadonlySet<string> = new Set<CategoryId>([
  "session",
  "power",
  "explorer",
  "appearance",
  "games",
  "setup",
  "storage",
  "notifications",
  "input",
]);

function normalizeAction(action: RawActionPresentation): ActionPresentation {
  if (!KNOWN_CATEGORIES.has(action.category)) {
    throw new SafeCoreError(
      "UNKNOWN_ACTION_PRESENTATION",
      "未登録のAction表示を受信したため、変更操作を停止しました。",
    );
  }
  const category = action.category as CategoryId;
  if (action.id === "games.process_watch" && action.currentState == null) {
    return {
      ...action,
      category,
      currentState: {
        kind: "unknown",
        label: "実行ファイル未登録",
        detail: "モードに実行ファイルを登録すると、起動したものが同じファイルかどうかを毎回たしかめます。",
      },
    };
  }
  return { ...action, category };
}
export class SafeCoreError extends Error {
  readonly code: string;
  readonly retryable: boolean;

  constructor(code: string, message: string, retryable = false) {
    super(message);
    this.name = "SafeCoreError";
    this.code = code;
    this.retryable = retryable;
  }
}

function normalizeCoreError(error: unknown): SafeCoreError {
  if (error instanceof SafeCoreError) {
    return error;
  }

  if (typeof error === "object" && error !== null) {
    const shape = error as CoreErrorShape;
    const code = typeof shape.code === "string" ? shape.code : "CORE_UNAVAILABLE";
    const message =
      typeof shape.userMessage === "string" && shape.userMessage.length <= 240
        ? shape.userMessage
        : "安全コアから応答がありません。変更操作は停止しました。";
    return new SafeCoreError(code, message, shape.retryable === true);
  }

  return new SafeCoreError(
    "CORE_UNAVAILABLE",
    "安全コアから応答がありません。変更操作は停止しました。",
    true,
  );
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw normalizeCoreError(error);
  }
}

export async function loadCoreSnapshot(): Promise<CoreSnapshot> {
  const [bootstrap, rawActions, timeline] = await Promise.all([
    call<BootstrapStatus>("get_bootstrap_status"),
    call<readonly RawActionPresentation[]>("list_actions"),
    call<readonly TimelineItem[]>("list_timeline"),
  ]);

  return { bootstrap, actions: rawActions.map(normalizeAction), timeline };
}

export function detectAction(actionId: string): Promise<DetectionResponse> {
  return call<DetectionResponse>("detect_action", { actionId });
}

export function previewActions(
  request: PreviewActionsRequest,
): Promise<PreviewResponse> {
  return call<PreviewResponse>("preview_actions", { request });
}

export function commitPreview(request: CommitPreviewRequest): Promise<CommitResult> {
  return call<CommitResult>("commit_preview", { request });
}

export function rollbackItem(request: RollbackItemRequest): Promise<CommitResult> {
  return call<CommitResult>("rollback_item", { request });
}

export function reconcileNow(): Promise<ReconcileResult> {
  return call<ReconcileResult>("reconcile_now");
}

export function listProfiles(): Promise<readonly StoredProfile[]> {
  return call<readonly StoredProfile[]>("profiles_list");
}

export function createProfile(request: CreateProfileRequest): Promise<StoredProfile> {
  return call<StoredProfile>("profile_create", { request });
}

export function setProfileEnabled(id: string, enabled: boolean): Promise<void> {
  return call<void>("profile_set_enabled", { id, enabled });
}

export function setProfileRibbonColor(id: string, color?: ModeRibbonColor): Promise<void> {
  return call<void>("profile_set_ribbon_color", { id, color: color ?? null });
}

export function runProfileNow(id: string): Promise<ManualProfileResult> {
  return call<ManualProfileResult>("profile_run_now", { id });
}

export function restoreProfileNow(id: string): Promise<ManualProfileResult> {
  return call<ManualProfileResult>("profile_restore_now", { id });
}
export function deleteProfile(id: string): Promise<void> {
  return call<void>("profile_delete", { id });
}

export function exportConfig(): Promise<string> {
  return call<string>("config_export");
}

export function importPreview(json: string): Promise<readonly ImportPreviewItem[]> {
  return call<readonly ImportPreviewItem[]>("config_import_preview", { json });
}

export function importApply(json: string): Promise<ImportResult> {
  return call<ImportResult>("config_import_apply", { json });
}

export function themeScheduleGet(): Promise<ThemeScheduleState> {
  return call<ThemeScheduleState>("theme_schedule_get");
}

export function themeScheduleSet(schedule: ThemeSchedule): Promise<ThemeScheduleState> {
  return call<ThemeScheduleState>("theme_schedule_set", { schedule });
}

export function tempCleanupPlan(): Promise<TempCleanupPlan> {
  return call<TempCleanupPlan>("storage_temp_cleanup_plan");
}

export function tempCleanupApply(): Promise<TempCleanupOutcome> {
  return call<TempCleanupOutcome>("storage_temp_cleanup_apply");
}

export function configSnapshotExport(): Promise<string> {
  return call<string>("config_snapshot_export");
}

export function openWindowsSettings(actionId: string): Promise<string> {
  return call<string>("open_windows_settings", { actionId });
}

export function getWindowLayoutStatus(): Promise<WindowLayoutStatus> {
  return call<WindowLayoutStatus>("get_window_layout_status");
}

export function saveWindowLayout(
  unregisteredGamesClosed: boolean,
): Promise<WindowLayoutStatus> {
  return call<WindowLayoutStatus>("save_window_layout", {
    unregisteredGamesClosed,
  });
}

export function listOffscreenWindows(): Promise<OffscreenWindowScan> {
  return call<OffscreenWindowScan>("list_offscreen_windows");
}

export function rescueOffscreenWindow(
  candidateId: string,
): Promise<OffscreenWindowUndo> {
  return call<OffscreenWindowUndo>("rescue_offscreen_window", { candidateId });
}

export function rollbackOffscreenWindow(
  undoId: string,
): Promise<OffscreenWindowUndo> {
  return call<OffscreenWindowUndo>("rollback_offscreen_window", { undoId });
}

export function setupCatalog(): Promise<readonly SetupAppDto[]> {
  return call<readonly SetupAppDto[]>("setup_app_catalog");
}

export function setupInstall(appId: string): Promise<InstallOutcome> {
  return call<InstallOutcome>("setup_app_install", { appId });
}

export function restartExplorerShell(): Promise<ShellRestartOutcome> {
  return call<ShellRestartOutcome>("restart_explorer_shell", {});
}

export function listTimeline(): Promise<readonly TimelineItem[]> {
  return call<readonly TimelineItem[]>("list_timeline");
}

/** ゲームの実行ファイルを利用者に選んでもらう。取り消しは null。 */
export function pickGameExecutable(): Promise<string | null> {
  return call<string | null>("pick_game_executable", {});
}

/** 試用として適用する。確定しなければ、次に開いたときに元へ戻る。 */
export function commitPreviewAsTrial(
  request: CommitPreviewRequest,
  holdSeconds: number,
): Promise<CommitResult> {
  return call<CommitResult>("commit_preview_as_trial", { request, holdSeconds });
}

/** 試用を確定する。以後この変更は自動で戻らない。 */
export function confirmTrial(transactionId: string): Promise<boolean> {
  return call<boolean>("confirm_trial", { transactionId });
}

/** 適用した設定が今も残っているかを照合する。読むだけで何も変更しない。 */
export function buildHealthReport(): Promise<HealthReport> {
  return call<HealthReport>("build_health_report");
}

/** 最大化中だけタスクバーを隠す設定の、いまの状態。 */
export function taskbarAutoHideState(): Promise<TaskbarAutoHideState> {
  return call<TaskbarAutoHideState>("taskbar_auto_hide_state");
}

/** その設定を切り替える。切ったときは監視側がタスクバーを戻す。 */
export function setTaskbarAutoHide(enabled: boolean): Promise<TaskbarAutoHideState> {
  return call<TaskbarAutoHideState>("set_taskbar_auto_hide", { enabled });
}

export function publicErrorMessage(error: unknown): string {
  if (error instanceof SafeCoreError) {
    return error.message;
  }
  return "処理を完了できませんでした。変更は成功として扱われていません。";
}

export function publicErrorCode(error: unknown): string {
  return error instanceof SafeCoreError ? error.code : "UNEXPECTED_UI_ERROR";
}
