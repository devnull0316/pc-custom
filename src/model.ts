export type DataMode = "loading" | "live" | "catalog";

export type ViewId = "home" | "actions" | "timeline" | "profiles" | "setup";

export type CategoryId =
  | "session"
  | "power"
  | "explorer"
  | "appearance"
  | "games"
  | "setup"
  | "storage"
  | "notifications"
  | "input";

export type RiskLevel = "safe" | "caution" | "experimental";

export type ActionKind = "persistent" | "session" | "one_way" | "observation" | "guided";

export type MethodClass =
  | "public_api"
  | "microsoft_cli"
  | "winget"
  | "official_module"
  | "documented_registry"
  | "limited_external"
  | "unverified_storage";

export type ActionAvailability =
  | "mutable"
  | "read_only"
  | "detect_only"
  | "blocked";

export type DetectionKind =
  | "known"
  | "unknown"
  | "unsupported"
  | "policy_managed"
  | "error";

export interface IntegrationState {
  installed: boolean;
  version: string | null;
  launchAvailable: boolean;
}

export interface ActionState {
  kind: DetectionKind;
  label: string;
  detail: string;
  items?: readonly string[];
  observedAt?: string;
  integration?: IntegrationState;
}

export interface ActionPresentation {
  id: string;
  actionVersion: number;
  name: string;
  description: string;
  audience: string;
  category: CategoryId;
  tags: readonly string[];
  supportedWindowsVersions: readonly string[];
  minimumBuild: number;
  maximumTestedBuild: number | null;
  riskLevel: RiskLevel;
  requiresAdmin: boolean;
  requiresRestart: boolean;
  requiresExplorerRestart: boolean;
  updateImpact: "low" | "review" | "high";
  reversible: boolean;
  kind: ActionKind;
  autoApplyEligible?: boolean;
  availability: ActionAvailability;
  methodClass?: MethodClass;
  methodSummary: string;
  desiredState: string;
  currentState?: ActionState | null;
  detailPoints: readonly string[];
  settingsPage?: string | null;
}

export interface BootstrapStatus {
  mode: "ready" | "read_only" | "recovery_required" | "unsupported";
  osLabel: string;
  build: number | null;
  message: string;
  recoveryCount: number;
}

export interface DetectionResponse {
  actionId: string;
  state: ActionState;
}

export type JsonValue = boolean | number | string | null | readonly JsonValue[] | { readonly [key: string]: JsonValue };

export interface PreviewActionRequest {
  actionId: string;
  parameters: Record<string, JsonValue>;
}

export interface PreviewActionsRequest {
  actions: readonly PreviewActionRequest[];
}

export interface PreviewChange {
  actionId: string;
  title: string;
  before: string;
  after: string;
  method: string;
  resourceLabel: string;
  riskLevel: RiskLevel;
  reversible: boolean;
}

export interface PreviewResponse {
  previewToken: string;
  expiresAt: string;
  osBuild: number;
  changes: readonly PreviewChange[];
  warnings: readonly string[];
}

export interface CommitPreviewRequest {
  previewToken: string;
}

/** 適用できた1件。itemId をそのまま rollbackItem へ渡せる。 */
export interface CommitItem {
  itemId: string;
  actionId: string;
  name: string;
}

export interface CommitResult {
  transactionId: string;
  status: "succeeded" | "rolled_back" | "recovery_required";
  message: string;
  details?: readonly string[];
  items: CommitItem[];
}

export type TimelineStatus =
  | "succeeded"
  | "rolled_back"
  | "rollback_failed"
  | "recovery_required"
  | "applying"
  | "rolling_back";

export interface TimelineStage {
  name: string;
  status: "complete" | "failed" | "pending";
}

export interface TimelineItem {
  itemId: string;
  transactionId: string;
  actionId: string;
  title: string;
  summary: string;
  status: TimelineStatus;
  startedAt: string;
  before: string;
  after: string;
  rollbackAvailable: boolean;
  retryAvailable: boolean;
  stages: readonly TimelineStage[];
  diagnosticId?: string | null;
}

export interface RollbackItemRequest {
  itemId: string;
}

export interface ReconcileResult {
  status: "clean" | "recovered" | "recovery_required";
  recoveredCount: number;
  remainingCount: number;
  message: string;
}

export interface CoreErrorShape {
  code?: string;
  userMessage?: string;
  retryable?: boolean;
}

export interface ResultTile {
  id: string;
  title: string;
  description: string;
  /** カテゴリ名のほか、履歴・モード・セットアップの各画面へも飛ばせる。 */
  category: CategoryId | "recovery" | "modes" | "setup-view";
  icon: IconName;
}

export type IconName =
  | "home"
  | "action"
  | "timeline"
  | "search"
  | "game"
  | "appearance"
  | "explorer"
  | "focus"
  | "power"
  | "recovery"
  | "sleep"
  | "check"
  | "warning"
  | "info"
  | "arrow"
  | "undo"
  | "plus"
  | "close"
  | "spinner"
  | "chevron";

export interface CategoryPresentation {
  id: CategoryId;
  label: string;
  description: string;
  icon: IconName;
}

export interface ProfileDraftItem {
  actionId: string;
  title: string;
}

export interface StoredProfileAction {
  actionId: string;
  parameters?: unknown;
}

export interface ManualRunRecord {
  transactionId: string;
  reversibleItemIds: readonly string[];
}

export type ModeRibbonColor = "sky" | "violet" | "mint" | "amber" | "rose";

export interface StoredProfile {
  id: string;
  name: string;
  executablePath?: string;
  volumeSerialNumber?: number;
  fileIdHex?: string;
  conflictPolicy: string;
  automationEnabled: boolean;
  actions: readonly StoredProfileAction[];
  ribbonColor?: ModeRibbonColor;
  activeRun?: ManualRunRecord;
}

export interface CreateProfileRequest {
  name: string;
  executablePath?: string;
  conflictPolicy?: string;
  actions: readonly StoredProfileAction[];
  ribbonColor?: ModeRibbonColor;
}

export interface ManualProfileResult {
  transactionId: string;
  status: "succeeded" | "rolled_back";
  reversibleItemCount: number;
  message: string;
}
export interface ImportPreviewItem {
  name: string;
  executablePath: string;
  actionCount: number;
  resolvable: boolean;
  note: string;
}

export interface ImportSkip {
  name: string;
  reason: string;
}

export interface ImportResult {
  imported: readonly string[];
  skipped: readonly ImportSkip[];
}

export interface ThemeSchedule {
  enabled: boolean;
  lightAtMinutes: number;
  darkAtMinutes: number;
}

export interface ThemeScheduleState {
  schedule: ThemeSchedule;
  lastError: string | null;
}

export type HotCornerAction = "none" | "open_modes";

export interface HotCornerSetting {
  topLeft: HotCornerAction;
  topRight: HotCornerAction;
  bottomLeft: HotCornerAction;
  bottomRight: HotCornerAction;
  dwellMs: number;
  cooldownMs: number;
}

export interface HotCornerState {
  setting: HotCornerSetting;
  lastError: string | null;
}

export interface TempCleanupCandidate {
  fileName: string;
  sizeBytes: number;
  ageDays: number;
}

export interface TempCleanupPlan {
  minAgeDays: number;
  candidates: readonly TempCleanupCandidate[];
  totalBytes: number;
  truncated: boolean;
}

export interface TempCleanupSkip {
  fileName: string;
  reason: string;
}

export interface TempCleanupOutcome {
  deletedCount: number;
  freedBytes: number;
  skipped: readonly TempCleanupSkip[];
  truncated: boolean;
}

export interface SetupAppDto {
  id: string;
  name: string;
  description: string;
  category: string;
  requiresAdmin: boolean;
}

export interface InstallOutcome {
  appId: string;
  appName: string;
  succeeded: boolean;
  exitCode: number | null;
  summary: string;
}

export interface WindowLayoutStatus {
  saved: boolean;
  snapshotId?: string | null;
  savedAtUnixMs?: number | null;
  windowCount: number;
  excludedGameWindows: number;
  skippedWindows: number;
}

export type OffscreenWindowBlockReason =
  | "higher_integrity"
  | "not_responding"
  | "access_unknown"
  | "already_rescued";

export interface OffscreenWindowCandidate {
  candidateId: string;
  applicationLabel: string;
  canRescue: boolean;
  unavailableReason: OffscreenWindowBlockReason | null;
}

export interface OffscreenWindowUndo {
  undoId: string;
  applicationLabel: string;
}

export interface OffscreenWindowScan {
  candidates: readonly OffscreenWindowCandidate[];
  excludedGameWindows: number;
  skippedWindows: number;
  undoItems: readonly OffscreenWindowUndo[];
}

export function isMutationAllowed(
  mode: DataMode,
  bootstrap: BootstrapStatus | null,
  action: ActionPresentation,
): boolean {
  return (
    mode === "live" &&
    bootstrap?.mode === "ready" &&
    action.availability === "mutable"
  );
}

export function riskLabel(risk: RiskLevel): string {
  switch (risk) {
    case "safe":
      return "低リスク";
    case "caution":
      return "注意";
    case "experimental":
      return "実験的";
  }
}

export function timelineStatusLabel(status: TimelineStatus): string {
  switch (status) {
    case "succeeded":
      return "適用済み";
    case "rolled_back":
      return "復元済み";
    case "rollback_failed":
      return "復元失敗";
    case "recovery_required":
      return "復旧が必要";
    case "applying":
      return "適用中";
    case "rolling_back":
      return "復元中";
  }
}

/** エクスプローラー(シェル)再起動の結果。 */
export interface ShellRestartOutcome {
  /** 終了させた explorer.exe の数。0 なら元から動いていなかった。 */
  terminated: number;
  /** タスクバーの復帰を確認できたか。false ならサインインし直しが要る。 */
  shellReturned: boolean;
  /** Windows の自動復帰では戻らず、こちらから起動し直したか。 */
  relaunched: boolean;
}

/** 適用した設定が今も残っているかの照合結果。原因は書かない。 */
export interface HealthEntry {
  actionId: string;
  name: string;
  note: string;
  appliedAt: string;
}

export interface HealthReport {
  hasBaseline: boolean;
  noBaselineNote: string | null;
  holding: readonly HealthEntry[];
  changed: readonly HealthEntry[];
  unknown: readonly HealthEntry[];
  updateReference: string | null;
  summary: string;
}

/** 最大化しているときだけタスクバーを隠す設定の状態。 */
export interface TaskbarAutoHideState {
  enabled: boolean;
  /** 誰かが手で変えたので、この機能が手を引いた。 */
  released: boolean;
  /** 元から常に隠す設定なので、この機能の出番が無い。 */
  notApplicable: boolean;
  lastError: string | null;
}
