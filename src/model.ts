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

export type ActionKind = "persistent" | "session" | "observation" | "guided";

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

export interface ActionState {
  kind: DetectionKind;
  label: string;
  detail: string;
  items?: readonly string[];
  observedAt?: string;
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

export interface PreviewActionRequest {
  actionId: string;
  parameters: Record<string, boolean | number | string>;
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

export interface CommitResult {
  transactionId: string;
  status: "succeeded" | "rolled_back" | "recovery_required";
  message: string;
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
  category: CategoryId | "recovery";
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

export interface StoredProfile {
  id: string;
  name: string;
  executablePath: string;
  volumeSerialNumber: number;
  fileIdHex: string;
  conflictPolicy: string;
  automationEnabled: boolean;
  actions: readonly StoredProfileAction[];
}

export interface CreateProfileRequest {
  name: string;
  executablePath: string;
  conflictPolicy?: string;
  actions: readonly StoredProfileAction[];
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
      return "安全";
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
