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
  ImportPreviewItem,
  ImportResult,
  InstallOutcome,
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
  TimelineItem,
} from "./model";

export interface CoreSnapshot {
  bootstrap: BootstrapStatus;
  actions: readonly ActionPresentation[];
  timeline: readonly TimelineItem[];
}

type RawActionPresentation = Omit<ActionPresentation, "category"> & { category: string };

const CATEGORY_BY_ACTION: Readonly<Record<string, CategoryId>> = {
  "session.prevent_sleep": "session",
  "power.active_scheme_check": "power",
  "power.active_scheme_switch": "power",
  "explorer.show_extensions": "explorer",
  "explorer.show_hidden": "explorer",
  "explorer.clock_seconds": "explorer",
  "appearance.transparency": "appearance",
  "taskbar.task_view": "appearance",
  "taskbar.widgets": "appearance",
  "explorer.item_checkboxes": "explorer",
  "explorer.compact_view": "explorer",
  "theme.color_mode": "appearance",
  "games.process_watch": "games",
  "taskbar.search_mode": "appearance",
  "taskbar.alignment": "appearance",
  "start.layout": "appearance",
  "start.recommendations": "appearance",
  "explorer.launch_target": "explorer",
  "explorer.recent_files": "explorer",
  "taskbar.button_grouping": "appearance",
  "taskbar.flashing": "appearance",
  "taskbar.share_window": "appearance",
  "taskbar.show_desktop": "appearance",
  "search.recent_on_hover": "appearance",
  "taskbar.multi_monitor": "appearance",
  "taskbar.multi_monitor_mode": "appearance",
  "taskbar.secondary_button_grouping": "appearance",
  "start.show_all_pins": "appearance",
  "start.recent_apps": "appearance",
  "appearance.accent_start_taskbar": "appearance",
  "appearance.accent_title_bars": "appearance",
  "appearance.auto_accent": "appearance",
  "games.game_mode": "games",
  "games.controller_game_bar": "games",
  "devices.autoplay": "setup",
  "notifications.usb_errors": "notifications",
  "notifications.weak_charger": "notifications",
  "input.autocorrect": "input",
  "input.double_space_period": "input",
  "input.auto_shift": "input",
  "input.voice_typing_key": "input",
  "input.multilingual_suggestions": "input",
  "explorer.status_bar": "explorer",
  "explorer.info_tips": "explorer",
  "explorer.hide_empty_drives": "explorer",
  "explorer.nav_expand_current": "explorer",
  "explorer.nav_show_all": "explorer",
  "explorer.separate_process": "explorer",
  "explorer.icons_only": "explorer",
  "explorer.drive_letters": "explorer",
  "explorer.preview_handlers": "explorer",
  "explorer.sharing_wizard": "explorer",
  "explorer.always_show_menus": "explorer",
  "appearance.taskbar_animations": "appearance",
  "notifications.toast_banners": "notifications",
  "setup.startup_inventory": "setup",
  "storage.free_space_check": "storage",
  "storage.temp_files_check": "storage",
  "games.readiness_check": "games",
};

function normalizeAction(action: RawActionPresentation): ActionPresentation {
  const category = CATEGORY_BY_ACTION[action.id];
  if (category === undefined) {
    throw new SafeCoreError(
      "UNKNOWN_ACTION_PRESENTATION",
      "未登録のAction表示を受信したため、変更操作を停止しました。",
    );
  }
  if (action.id === "games.process_watch" && action.currentState == null) {
    return {
      ...action,
      category,
      currentState: {
        kind: "unknown",
        label: "実行ファイル未登録",
        detail: "プロファイルで実行ファイルを登録した後に、本人性を照合します。",
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

export function setupCatalog(): Promise<readonly SetupAppDto[]> {
  return call<readonly SetupAppDto[]>("setup_app_catalog");
}

export function setupInstall(appId: string): Promise<InstallOutcome> {
  return call<InstallOutcome>("setup_app_install", { appId });
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
