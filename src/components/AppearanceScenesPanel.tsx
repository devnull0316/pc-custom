import type { CSSProperties } from "react";

import {
  APPEARANCE_SCENE_ACTION_IDS,
  APPEARANCE_SCENES,
} from "../appearanceScenes";
import type {
  ActionPresentation,
  BootstrapStatus,
  DataMode,
} from "../model";
import { Icon } from "./Icon";

interface AppearanceScenesPanelProps {
  actions: readonly ActionPresentation[];
  bootstrap: BootstrapStatus | null;
  dataMode: DataMode;
  previewPendingKey: string | null;
  onPreview: (sceneId: string) => void;
}

export function AppearanceScenesPanel({
  actions,
  bootstrap,
  dataMode,
  previewPendingKey,
  onPreview,
}: AppearanceScenesPanelProps) {
  const targetsAreMutable = APPEARANCE_SCENE_ACTION_IDS.every((actionId) =>
    actions.some((action) => action.id === actionId && action.availability === "mutable"),
  );
  const canPreview =
    dataMode === "live"
    && bootstrap?.mode === "ready"
    && targetsAreMutable;

  return (
    <section aria-label="配色シーン" className="appearance-scenes">
      <header className="appearance-scenes__header">
        <div>
          <p className="eyebrow">3つの変更をまとめて試す</p>
          <h2>配色シーン</h2>
          <p>
            明暗、透過効果、ウィンドウ色をひとまとまりでプレビューします。
            選んだだけでは変更されません。
          </p>
        </div>
        <p className="appearance-scenes__estimate">
          <Icon name="info" size={15} />
          色見本は概算です。Windowsやアプリごとに見え方が異なります。
        </p>
      </header>

      <div className="appearance-scenes__grid">
        {APPEARANCE_SCENES.map((scene) => {
          const pendingKey = `appearance-scene:${scene.id}`;
          const previewing = previewPendingKey === pendingKey;
          const style = {
            "--scene-surface": scene.swatch.surface,
            "--scene-accent": scene.swatch.accent,
          } as CSSProperties;
          return (
            <article className="appearance-scene-card" key={scene.id}>
              <div
                aria-hidden="true"
                className={`appearance-scene-card__swatch${scene.swatch.translucent ? " appearance-scene-card__swatch--translucent" : ""}`}
                style={style}
              >
                <span />
                <span />
                <span />
              </div>
              <div className="appearance-scene-card__copy">
                <h3>{scene.name}</h3>
                <p>{scene.description}</p>
                <ul>
                  {scene.details.map((detail) => <li key={detail}>{detail}</li>)}
                </ul>
              </div>
              <button
                className="secondary-button"
                disabled={!canPreview || previewPendingKey !== null}
                onClick={() => onPreview(scene.id)}
                type="button"
              >
                {previewing
                  ? <Icon className="spin" name="spinner" />
                  : <Icon name="arrow" />}
                {previewing ? "プレビュー作成中" : "変更内容をプレビュー"}
              </button>
            </article>
          );
        })}
      </div>

      {canPreview ? null : (
        <p className="appearance-scenes__unavailable">
          <Icon name="info" size={15} />
          3つの変更を安全コアで利用できるときだけ、シーンをプレビューできます。
        </p>
      )}
    </section>
  );
}
