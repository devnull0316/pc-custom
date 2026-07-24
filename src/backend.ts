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
  "explorer.show_extensions": "explorer",
  "explorer.show_hidden": "explorer",
  "explorer.clock_seconds": "explorer",
  "theme.color_mode": "appearance",
  "games.process_watch": "games",
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

export function publicErrorMessage(error: unknown): string {
  if (error instanceof SafeCoreError) {
    return error.message;
  }
  return "処理を完了できませんでした。変更は成功として扱われていません。";
}

export function publicErrorCode(error: unknown): string {
  return error instanceof SafeCoreError ? error.code : "UNEXPECTED_UI_ERROR";
}
