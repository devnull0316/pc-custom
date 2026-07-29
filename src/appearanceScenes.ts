import rawScenes from "./appearance-scenes.json";
import type { JsonValue, PreviewActionsRequest } from "./model";

export const APPEARANCE_SCENE_ACTION_IDS = [
  "theme.color_mode",
  "appearance.transparency",
  "appearance.window_color",
] as const;

export type AppearanceSceneActionId = (typeof APPEARANCE_SCENE_ACTION_IDS)[number];

export interface AppearanceScene {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly details: readonly string[];
  readonly swatch: {
    readonly surface: string;
    readonly accent: string;
    readonly translucent: boolean;
  };
  readonly actions: readonly {
    readonly actionId: AppearanceSceneActionId;
    readonly parameters: Record<string, JsonValue>;
  }[];
}

const ACTION_ID_SET = new Set<string>(APPEARANCE_SCENE_ACTION_IDS);
const HEX_COLOR = /^#[0-9a-f]{6}$/iu;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readString(value: unknown, field: string): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`配色シーンの${field}が不正です。`);
  }
  return value;
}

function readParameters(
  actionId: AppearanceSceneActionId,
  value: unknown,
): Record<string, JsonValue> {
  if (!isRecord(value)) throw new Error(`配色シーン ${actionId} の引数が不正です。`);

  const keys = Object.keys(value);
  if (actionId === "theme.color_mode") {
    if (
      keys.length !== 1
      || keys[0] !== "mode"
      || (value.mode !== "light" && value.mode !== "dark")
    ) {
      throw new Error("配色シーンの明暗指定がActionの引数形式と一致しません。");
    }
  } else if (actionId === "appearance.transparency") {
    if (keys.length !== 1 || keys[0] !== "enabled" || typeof value.enabled !== "boolean") {
      throw new Error("配色シーンの透過指定がActionの引数形式と一致しません。");
    }
  } else {
    const colors = new Set([
      "windows_blue",
      "teal",
      "purple",
      "green",
      "amber",
      "red",
      "graphite",
    ]);
    if (keys.length !== 1 || keys[0] !== "color" || !colors.has(value.color as string)) {
      throw new Error("配色シーンの色指定がActionの引数形式と一致しません。");
    }
  }
  return value as Record<string, JsonValue>;
}

function readScene(value: unknown): AppearanceScene {
  if (!isRecord(value)) throw new Error("配色シーンの定義が不正です。");
  if (!Array.isArray(value.details) || value.details.length !== 3) {
    throw new Error("配色シーンの変更内容は3件である必要があります。");
  }
  if (!Array.isArray(value.actions) || value.actions.length !== 3) {
    throw new Error("配色シーンのActionは3件である必要があります。");
  }
  if (!isRecord(value.swatch)) throw new Error("配色シーンの色見本が不正です。");

  const seen = new Set<string>();
  const actions = value.actions.map((actionValue) => {
    if (!isRecord(actionValue) || !ACTION_ID_SET.has(actionValue.actionId as string)) {
      throw new Error("配色シーンに未承認のActionが含まれています。");
    }
    const actionId = actionValue.actionId as AppearanceSceneActionId;
    if (seen.has(actionId)) throw new Error("配色シーンに同じActionが重複しています。");
    seen.add(actionId);
    return {
      actionId,
      parameters: readParameters(actionId, actionValue.parameters),
    };
  });
  if (APPEARANCE_SCENE_ACTION_IDS.some((actionId) => !seen.has(actionId))) {
    throw new Error("配色シーンに必要なActionが不足しています。");
  }

  const surface = readString(value.swatch.surface, "背景色");
  const accent = readString(value.swatch.accent, "アクセント色");
  if (!HEX_COLOR.test(surface) || !HEX_COLOR.test(accent)) {
    throw new Error("配色シーンの色見本は固定の16進色で指定してください。");
  }
  if (typeof value.swatch.translucent !== "boolean") {
    throw new Error("配色シーンの透過見本が不正です。");
  }

  return {
    id: readString(value.id, "ID"),
    name: readString(value.name, "名前"),
    description: readString(value.description, "説明"),
    details: value.details.map((detail) => readString(detail, "変更内容")),
    swatch: {
      surface,
      accent,
      translucent: value.swatch.translucent,
    },
    actions,
  };
}

if (!Array.isArray(rawScenes) || rawScenes.length < 3 || rawScenes.length > 4) {
  throw new Error("配色シーンは3〜4件で定義してください。");
}

export const APPEARANCE_SCENES: readonly AppearanceScene[] = rawScenes.map(readScene);

export function appearanceSceneRequest(sceneId: string): PreviewActionsRequest {
  const scene = APPEARANCE_SCENES.find((candidate) => candidate.id === sceneId);
  if (scene === undefined) throw new Error("未登録の配色シーンです。");
  return {
    actions: scene.actions.map((action) => ({
      actionId: action.actionId,
      parameters: action.parameters,
    })),
  };
}
