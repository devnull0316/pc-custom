import type {
  ActionPresentation,
  CategoryId,
  CategoryPresentation,
  CommitItem,
  DataMode,
} from "./model";
import { screenText } from "./publicCopy";

/**
 * 非同期読み取りのうち、最後に開始した要求だけが画面を更新できるようにする。
 * プレビューと履歴で同じ世代管理を共有し、完了順による上書きを防ぐ。
 */
export class LatestRequestGuard {
  private generation = 0;

  begin(): number {
    this.generation += 1;
    return this.generation;
  }

  invalidate(): void {
    this.generation += 1;
  }

  isCurrent(generation: number): boolean {
    return generation === this.generation;
  }
}

export type ReadState<T> =
  | { readonly status: "loading"; readonly value: T }
  | { readonly status: "ready"; readonly value: T }
  | { readonly status: "error"; readonly value: T; readonly message: string };

export function loadingRead<T>(value: T): ReadState<T> {
  return { status: "loading", value };
}

export function successfulRead<T>(value: T): ReadState<T> {
  return { status: "ready", value };
}

/** 読み取り失敗時は、既存値を空配列や編集可能な既定値へ置き換えない。 */
export function failedRead<T>(current: ReadState<T>, message: string): ReadState<T> {
  return { status: "error", value: current.value, message };
}

export function selectDisplayedAction(
  displayedActions: readonly ActionPresentation[],
  selectedActionId: string | null,
): ActionPresentation | undefined {
  return displayedActions.find((action) => action.id === selectedActionId) ?? displayedActions[0];
}

export interface ImportPreviewSnapshot<T> {
  readonly source: string;
  readonly items: readonly T[];
}

export function isCurrentImportPreview<T>(
  preview: ImportPreviewSnapshot<T> | null,
  input: string,
): preview is ImportPreviewSnapshot<T> {
  return preview !== null && preview.source === input;
}

export function canRunLiveMutation(dataMode: DataMode): boolean {
  return dataMode === "live";
}

export type ErrorStateUpdate<T> =
  | { readonly kind: "clear" }
  | { readonly kind: "show"; readonly error: T };

/** エラーは新しい失敗で上書きし、明示的に閉じたときだけ消す。 */
export function updateErrorState<T>(
  _current: T | null,
  update: ErrorStateUpdate<T>,
): T | null {
  if (update.kind === "clear") return null;
  return update.error;
}

export interface ActionBrowserState {
  readonly categoryActions: readonly ActionPresentation[];
  readonly displayedActions: readonly ActionPresentation[];
  readonly isSearching: boolean;
  readonly selectedAction: ActionPresentation | undefined;
}

/** 絞り込み後の一覧と詳細選択を同じスナップショットから決める。 */
export function deriveActionBrowserState(
  actions: readonly ActionPresentation[],
  selectedCategory: CategoryId,
  selectedActionId: string | null,
  searchQuery: string,
  categories: readonly CategoryPresentation[],
): ActionBrowserState {
  const categoryActions = actions.filter((action) => action.category === selectedCategory);
  const normalizedQuery = searchQuery.trim().toLowerCase();
  const isSearching = normalizedQuery.length > 0;
  const displayedActions = isSearching
    ? actions.filter((action) => {
        const category = categories.find((candidate) => candidate.id === action.category);
        const categoryLabel = category?.label.toLowerCase() ?? "";
        const categoryDescription = category?.description.toLowerCase() ?? "";
        return action.name.toLowerCase().includes(normalizedQuery)
          || action.description.toLowerCase().includes(normalizedQuery)
          || screenText(action.description, "").toLowerCase().includes(normalizedQuery)
          || action.tags.some((tag) => tag.toLowerCase().includes(normalizedQuery))
          || action.category.toLowerCase().includes(normalizedQuery)
          || categoryLabel.includes(normalizedQuery)
          || categoryDescription.includes(normalizedQuery);
      })
    : categoryActions;

  return {
    categoryActions,
    displayedActions,
    isSearching,
    selectedAction: selectDisplayedAction(displayedActions, selectedActionId),
  };
}

export function withoutCommitItem(
  items: readonly CommitItem[],
  completedItemId: string,
): CommitItem[] {
  return items.filter((item) => item.itemId !== completedItemId);
}

export type JustAppliedUpdate =
  | { readonly kind: "replace"; readonly items: readonly CommitItem[] }
  | { readonly kind: "remove"; readonly itemId: string }
  | { readonly kind: "clear" };

/** 直前適用バーの増減を、適用結果とrollback結果だけから決める。 */
export function updateJustApplied(
  current: readonly CommitItem[],
  update: JustAppliedUpdate,
): CommitItem[] {
  if (update.kind === "clear") return [];
  if (update.kind === "remove") return withoutCommitItem(current, update.itemId);
  return [...update.items];
}

export function trialConfirmationSucceeded(result: boolean): boolean {
  return result === true;
}

export function shouldClearProfileDraft(created: boolean): boolean {
  return created;
}

export interface StorageTimestamp {
  readonly dateTime: string;
  readonly label: string;
}

export function formatStorageTimestamp(unixMs: number): StorageTimestamp | null {
  if (!Number.isFinite(unixMs)) return null;
  const date = new Date(unixMs);
  if (!Number.isFinite(date.getTime())) return null;
  try {
    return {
      dateTime: date.toISOString(),
      label: date.toLocaleString("ja-JP"),
    };
  } catch {
    return null;
  }
}
