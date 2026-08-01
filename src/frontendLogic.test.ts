import { it, expect } from "vitest";
import type { ActionPresentation, CategoryPresentation, CommitItem } from "./model";
import {
  LatestRequestGuard,
  canRunLiveMutation,
  deriveActionBrowserState,
  failedRead,
  formatStorageTimestamp,
  isCurrentImportPreview,
  loadingRead,
  selectDisplayedAction,
  shouldClearProfileDraft,
  successfulRead,
  trialConfirmationSucceeded,
  updateErrorState,
  updateJustApplied,
  withoutCommitItem,
} from "./frontendLogic";

interface RegressionCase {
  readonly name: string;
  readonly run: () => void;
}

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

function action(
  id: string,
  overrides: Partial<ActionPresentation> = {},
): ActionPresentation {
  return {
    id,
    actionVersion: 1,
    name: id,
    description: id,
    audience: "test",
    category: "session",
    tags: [],
    supportedWindowsVersions: ["11"],
    minimumBuild: 0,
    maximumTestedBuild: null,
    riskLevel: "safe",
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    updateImpact: "low",
    reversible: true,
    kind: "persistent",
    availability: "mutable",
    methodSummary: "test",
    desiredState: "test",
    detailPoints: [],
    ...overrides,
  };
}

const firstCommit: CommitItem = { itemId: "first", actionId: "a", name: "A" };
const secondCommit: CommitItem = { itemId: "second", actionId: "b", name: "B" };
const categories: readonly CategoryPresentation[] = [
  { id: "session", label: "集中", description: "その場だけ使う", icon: "focus" },
  { id: "power", label: "電源", description: "電池と電源モード", icon: "power" },
];

const cases: readonly RegressionCase[] = [
  {
    name: "1: 絞り込み外の選択ではなく表示中の先頭を詳細に出す",
    run: () => {
      const visible = [action("visible")];
      assert(selectDisplayedAction(visible, "filtered-out")?.id === "visible", "絞り込み外のActionが残った");
    },
  },
  {
    name: "2: 古いプレビュー要求は新しい要求を上書きできない",
    run: () => {
      const guard = new LatestRequestGuard();
      const oldRequest = guard.begin();
      const newRequest = guard.begin();
      assert(!guard.isCurrent(oldRequest) && guard.isCurrent(newRequest), "プレビュー世代が逆転した");
    },
  },
  {
    name: "3: confirmTrial=falseを保存成功として扱わない",
    run: () => assert(!trialConfirmationSucceeded(false), "falseを保存成功として扱った"),
  },
  {
    name: "4: 復元済み項目を直前適用一覧から直ちに除く",
    run: () => {
      const remaining = withoutCommitItem([firstCommit, secondCommit], "second");
      assert(remaining.length === 1 && remaining[0]?.itemId === "first", "復元済み項目が残った");
    },
  },
  {
    name: "5: 確認時と異なるJSONは取り込めない",
    run: () => {
      const preview = { source: "confirmed", items: ["mode"] };
      assert(!isCurrentImportPreview(preview, "changed"), "未確認JSONが確認済みになった");
    },
  },
  {
    name: "6: テーマ取得失敗を編集可能な既定値へ置き換えない",
    run: () => {
      const failed = failedRead(loadingRead<null>(null), "read failed");
      assert(failed.status === "error" && failed.value === null, "テーマの既定値を捏造した");
    },
  },
  {
    name: "7: catalogでは削除処理を実行不可にする",
    run: () => assert(!canRunLiveMutation("catalog"), "catalogで変更処理を許可した"),
  },
  {
    name: "8: モード取得失敗時も直前に読めた一覧を保持する",
    run: () => {
      const previous = successfulRead(["registered-mode"]);
      const failed = failedRead(previous, "read failed");
      assert(failed.value[0] === "registered-mode" && failed.status === "error", "モード一覧を空にした");
    },
  },
  {
    name: "9: モード作成失敗時はフォームを消さない",
    run: () => assert(!shouldClearProfileDraft(false), "失敗時にフォームを消した"),
  },
  {
    name: "10: 古い容量履歴の取得結果を無効化する",
    run: () => {
      const guard = new LatestRequestGuard();
      const initialLoad = guard.begin();
      guard.invalidate();
      assert(!guard.isCurrent(initialLoad), "古い容量履歴が有効なままになった");
    },
  },
  {
    name: "11: 範囲外日時は描画用ISO文字列へ変換しない",
    run: () => assert(formatStorageTimestamp(Number.MAX_VALUE) === null, "範囲外日時を受理した"),
  },
  {
    name: "12: 読み取り開始は値を保ったloading状態になる",
    run: () => {
      const state = loadingRead(["previous"]);
      assert(state.status === "loading" && state.value[0] === "previous", "loading状態を作れなかった");
    },
  },
  {
    name: "13: 読み取り成功は取得値を持つready状態になる",
    run: () => {
      const state = successfulRead(["fresh"]);
      assert(state.status === "ready" && state.value[0] === "fresh", "ready状態を作れなかった");
    },
  },
  {
    name: "14: 有効な日時は機械可読値と表示値の両方になる",
    run: () => {
      const timestamp = formatStorageTimestamp(0);
      assert(
        timestamp?.dateTime === "1970-01-01T00:00:00.000Z" && timestamp.label.length > 0,
        "有効な日時を描画用へ変換できなかった",
      );
    },
  },
  {
    name: "15: liveでは変更処理を実行できる",
    run: () => assert(canRunLiveMutation("live"), "liveの変更処理まで停止した"),
  },
  {
    name: "16: 新しいエラーを表示できる",
    run: () => assert(updateErrorState(null, { kind: "show", error: "first" }) === "first", "エラーを表示できなかった"),
  },
  {
    name: "17: 表示中のエラーを明示操作で消せる",
    run: () => assert(updateErrorState("first", { kind: "clear" }) === null, "エラーを消せなかった"),
  },
  {
    name: "18: 後から発生したエラーで古いエラーを上書きする",
    run: () => assert(updateErrorState("first", { kind: "show", error: "second" }) === "second", "古いエラーが残った"),
  },
  {
    name: "19: 絞り込み後は一覧内のActionだけを詳細選択する",
    run: () => {
      const state = deriveActionBrowserState(
        [action("old", { category: "session" }), action("power", { category: "power" })],
        "session",
        "old",
        "電源",
        categories,
      );
      assert(
        state.isSearching
          && state.displayedActions.length === 1
          && state.selectedAction?.id === "power",
        "絞り込み外の選択が詳細に残った",
      );
    },
  },
  {
    name: "20: 検索していない一覧は選択カテゴリだけに絞る",
    run: () => {
      const state = deriveActionBrowserState(
        [action("session"), action("power", { category: "power" })],
        "power",
        "session",
        "   ",
        categories,
      );
      assert(
        !state.isSearching
          && state.categoryActions.length === 1
          && state.displayedActions[0]?.id === "power"
          && state.selectedAction?.id === "power",
        "カテゴリ外のActionが一覧または詳細に残った",
      );
    },
  },
  {
    name: "21: 新しい適用結果で直前適用一覧を置き換える",
    run: () => {
      const updated = updateJustApplied([firstCommit], { kind: "replace", items: [secondCommit] });
      assert(updated.length === 1 && updated[0]?.itemId === "second", "新しい適用結果へ置き換わらなかった");
    },
  },
  {
    name: "22: rollback済みの1件だけを直前適用一覧から減らす",
    run: () => {
      const updated = updateJustApplied([firstCommit, secondCommit], { kind: "remove", itemId: "first" });
      assert(updated.length === 1 && updated[0]?.itemId === "second", "rollback対象以外も消えた、または対象が残った");
    },
  },
  {
    name: "23: 案内を閉じたら直前適用一覧を空にする",
    run: () => assert(updateJustApplied([firstCommit], { kind: "clear" }).length === 0, "直前適用一覧を消せなかった"),
  },
];

// ここで直接 run() すると、読み込み時に例外が出てファイルごと収集に失敗する。
// どのケースが守っているのか分からなくなるので、必ずランナーの中で走らせる。

// ケース配列だけでは、どのテストランナーも拾わない。
// 実際に走らない形で置かれていて、11件が一度も実行されていなかった。
// **走らないテストは、無いのと同じ。**
for (const testCase of cases) {
  it(testCase.name, () => {
    expect(() => testCase.run()).not.toThrow();
  });
}
