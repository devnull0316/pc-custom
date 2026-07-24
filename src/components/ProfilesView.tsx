import { useMemo, useState } from "react";

import type {
  ActionPresentation,
  CreateProfileRequest,
  DataMode,
  StoredProfile,
} from "../model";
import { Icon } from "./Icon";

interface ProfilesViewProps {
  dataMode: DataMode;
  profiles: readonly StoredProfile[];
  actions: readonly ActionPresentation[];
  busy: boolean;
  onCreate: (request: CreateProfileRequest) => void;
  onSetEnabled: (id: string, enabled: boolean) => void;
  onDelete: (id: string) => void;
  onOpenActions: () => void;
}

export function ProfilesView({
  dataMode,
  profiles,
  actions,
  busy,
  onCreate,
  onSetEnabled,
  onDelete,
  onOpenActions,
}: ProfilesViewProps) {
  const [name, setName] = useState("");
  const [exePath, setExePath] = useState("");
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());

  const live = dataMode === "live";
  // core metadataが明示的に許可したActionだけを自動適用候補へ出す。
  const selectable = useMemo(
    () => actions.filter(
      (action) => action.autoApplyEligible === true
        && (action.kind === "persistent" || action.kind === "session"),
    ),
    [actions],
  );

  const canSubmit =
    live && !busy && name.trim().length > 0 && exePath.trim().length > 0;

  function toggle(id: string) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function submit() {
    if (!canSubmit) return;
    onCreate({
      name: name.trim(),
      executablePath: exePath.trim(),
      actions: [...selected].map((actionId) => ({ actionId })),
    });
    setName("");
    setExePath("");
    setSelected(new Set());
  }

  return (
    <section className="profiles-view">
      <header className="view-header">
        <span className="eyebrow">ゲームを快適にする</span>
        <h1>ゲームプロファイル</h1>
        <p>
          登録したゲームの起動を検知したら、選んだ準備を自動でまとめ、終了したら変更した項目だけを元へ戻します。
          ゲームを速くする機能ではなく、通知やスリープなどの<strong>邪魔と設定ミスを減らす</strong>ためのものです。
        </p>
      </header>

      {live ? null : (
        <div className="inline-note" role="note">
          <Icon name="info" />
          <span>閲覧モードです。安全コアに接続すると、プロファイルの作成・有効化ができます。</span>
        </div>
      )}

      <div className="profiles-layout">
        <form
          className="profile-create"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <h2>新しいプロファイル</h2>
          <label className="field">
            <span>名前</span>
            <input
              disabled={!live || busy}
              maxLength={120}
              onChange={(event) => setName(event.target.value)}
              placeholder="例: VALORANT"
              type="text"
              value={name}
            />
          </label>
          <label className="field">
            <span>ゲームの実行ファイル（.exe のフルパス）</span>
            <input
              disabled={!live || busy}
              onChange={(event) => setExePath(event.target.value)}
              placeholder={String.raw`例: C:\Riot Games\VALORANT\live\VALORANT.exe`}
              spellCheck={false}
              type="text"
              value={exePath}
            />
            <small>ローカルドライブ上の実在する実行ファイルだけを登録できます。本人性（ファイル識別子）も記録します。</small>
          </label>

          <fieldset className="field">
            <legend>起動時にまとめる準備</legend>
            {selectable.length === 0 ? (
              <p className="muted">
                選べるActionがありません。<button className="link-button" onClick={onOpenActions} type="button">Action一覧</button>を確認してください。
              </p>
            ) : (
              <div className="action-picker">
                {selectable.map((action) => (
                  <label className="action-pick" key={action.id}>
                    <input
                      checked={selected.has(action.id)}
                      disabled={!live || busy}
                      onChange={() => toggle(action.id)}
                      type="checkbox"
                    />
                    <span>
                      <strong>{action.name}</strong>
                      <small>{action.description}</small>
                    </span>
                  </label>
                ))}
              </div>
            )}
          </fieldset>

          <button className="primary-button" disabled={!canSubmit} type="submit">
            {busy ? <Icon className="spin" name="spinner" /> : <Icon name="plus" />}
            プロファイルを作成
          </button>
          <p className="muted small">
            作成しても自動適用はまだ開始しません。各プロファイルの「自動適用」を有効にしたときだけ働きます。
          </p>
        </form>

        <div className="profile-list-panel">
          <h2>登録済みプロファイル</h2>
          {profiles.length === 0 ? (
            <div className="empty-block">
              <Icon name="game" />
              <strong>まだプロファイルはありません</strong>
              <span>左のフォームから、よく遊ぶゲームを登録してみましょう。</span>
            </div>
          ) : (
            <ul className="profile-list">
              {profiles.map((profile) => (
                <li className="profile-card" key={profile.id}>
                  <div className="profile-card__head">
                    <div>
                      <strong>{profile.name}</strong>
                      <code title={profile.executablePath}>{fileName(profile.executablePath)}</code>
                    </div>
                    <span
                      className={`profile-state profile-state--${
                        profile.automationEnabled ? "on" : "off"
                      }`}
                    >
                      {profile.automationEnabled ? "自動適用オン" : "自動適用オフ"}
                    </span>
                  </div>
                  <div className="profile-card__actions-count">
                    <Icon name="action" size={15} />
                    <span>{profile.actions.length}件の準備</span>
                  </div>
                  <div className="profile-card__controls">
                    <button
                      className="secondary-button"
                      disabled={!live || busy}
                      onClick={() => onSetEnabled(profile.id, !profile.automationEnabled)}
                      type="button"
                    >
                      {profile.automationEnabled ? "自動適用を止める" : "自動適用を有効にする"}
                    </button>
                    <button
                      aria-label={`${profile.name}を削除`}
                      className="icon-button"
                      disabled={!live || busy}
                      onClick={() => onDelete(profile.id)}
                      type="button"
                    >
                      <Icon name="close" />
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </section>
  );
}

function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}
