import type { ActionPresentation, BootstrapStatus } from "./model";

/**
 * いまのPCを見て、変えたほうがよいことを数件だけ出す。
 *
 * 設計の約束:
 * - **検出した事実を言えるものだけを出す。** 「速くなります」のように根拠を示せないものは入れない。
 * - この環境で実際に変更できるものだけを出す。押しても何も起きない提案はしない。
 * - 件数を絞る。67項目から選ばせるのが辛いのに、推奨が20件あっても同じことになる。
 */
export interface Recommendation {
  /** 対象 Action。押すとその項目へ移動する。 */
  actionId: string;
  /** 何をするか。結果で書く。 */
  title: string;
  /** なぜ出したか。**検出した事実**を書く。ここが書けないものは推奨にしない。 */
  reason: string;
  /** 先に出すもの（小さいほど先）。 */
  weight: number;
}

const MAX_SHOWN = 4;

function find(actions: readonly ActionPresentation[], id: string): ActionPresentation | undefined {
  return actions.find((action) => action.id === id);
}

/** この環境で実際に変更できるか。できないものは提案しない。 */
function changeable(action: ActionPresentation | undefined): action is ActionPresentation {
  return action !== undefined && action.availability === "mutable";
}

export function buildRecommendations(
  actions: readonly ActionPresentation[],
  bootstrap: BootstrapStatus | null,
): Recommendation[] {
  const found: Recommendation[] = [];

  // 未復元の変更は、何よりも先に片付けるべきもの。
  // 中途半端に適用された状態のまま新しい変更を重ねさせない。
  if (bootstrap !== null && bootstrap.recoveryCount > 0) {
    found.push({
      actionId: "__recovery",
      title: "元に戻せていない変更を片付ける",
      reason: `前回の変更が${bootstrap.recoveryCount}件、途中で止まったままです。`,
      weight: 0,
    });
  }

  // 拡張子が隠れていると、ファイルの種類が見分けられない。
  // Windows の既定は「隠す」なので、多くの人がこの状態にある。
  const extensions = find(actions, "explorer.show_extensions");
  if (changeable(extensions) && extensions.currentState?.label?.includes("隠") === true) {
    found.push({
      actionId: extensions.id,
      title: "ファイルの種類が見分けられるようにする",
      reason: "いま拡張子が隠れています。見た目が同じファイルの区別がつきません。",
      weight: 1,
    });
  }

  // 空き容量。数字が出ているときだけ言う。
  const space = find(actions, "storage.free_space_check");
  const spaceLabel = space?.currentState?.label ?? "";
  const lowSpace = /([0-9]+(?:\.[0-9]+)?)\s*GB/.exec(spaceLabel);
  if (lowSpace !== null && Number(lowSpace[1]) < 20) {
    found.push({
      actionId: "storage.temp_files_check",
      title: "古い一時ファイルを消して容量を空ける",
      reason: `システムドライブの空きが ${lowSpace[1]} GB しかありません。`,
      weight: 2,
    });
  }

  // タスクバーの検索ボックスは横幅を大きく取る。実機で反映を確認済みの項目。
  const search = find(actions, "taskbar.search_mode");
  if (changeable(search) && search.currentState?.label?.includes("ボックス") === true) {
    found.push({
      actionId: search.id,
      title: "タスクバーの検索を小さくする",
      reason: "いま検索ボックスが横幅を占めています。アイコンだけにすると、その分タスクバーを広く使えます。",
      weight: 3,
    });
  }

  // PowerToys は未導入のときだけ。導入済みの人に勧めても意味がない。
  const powerToys = find(actions, "setup.powertoys_status");
  if (powerToys?.currentState?.integration?.installed === false) {
    found.push({
      actionId: powerToys.id,
      title: "PowerToys を入れて普段の操作を楽にする",
      reason: "まだ入っていません。キー割り当てや書式なし貼り付けなどを Microsoft 公式が提供しています。",
      weight: 4,
    });
  }

  return found.sort((left, right) => left.weight - right.weight).slice(0, MAX_SHOWN);
}
