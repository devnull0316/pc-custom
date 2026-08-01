import { it, expect } from "vitest";
import type { ActionPresentation, CommitItem } from "./model";
import {
  LatestRequestGuard,
  canRunLiveMutation,
  failedRead,
  formatStorageTimestamp,
  isCurrentImportPreview,
  loadingRead,
  selectDisplayedAction,
  shouldClearProfileDraft,
  successfulRead,
  trialConfirmationSucceeded,
  withoutCommitItem,
} from "./frontendLogic";

interface RegressionCase {
  readonly name: string;
  readonly run: () => void;
}

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

function action(id: string): ActionPresentation {
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
  };
}

const firstCommit: CommitItem = { itemId: "first", actionId: "a", name: "A" };
const secondCommit: CommitItem = { itemId: "second", actionId: "b", name: "B" };

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
