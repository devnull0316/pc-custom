import { useEffect, useMemo, useState } from "react";

import { exportConfig, getWindowLayoutStatus, importApply, importPreview, pickGameExecutable, publicErrorMessage } from "../backend";
import type {
  ActionPresentation,
  CreateProfileRequest,
  DataMode,
  ImportPreviewItem,
  JsonValue,
  ModeRibbonColor,
  StoredProfile,
  WindowLayoutStatus,
} from "../model";
import { GameReadinessPanel } from "./GameReadinessPanel";
import { HotCornerPanel } from "./HotCornerPanel";
import { Icon } from "./Icon";
import { ShareSessionPanel } from "./ShareSessionPanel";

interface ProfilesViewProps {
  dataMode: DataMode;
  profiles: readonly StoredProfile[];
  actions: readonly ActionPresentation[];
  busy: boolean;
  onCreate: (request: CreateProfileRequest) => void;
  onParametersForAction: (actionId: string) => Record<string, JsonValue>;
  onRun: (id: string) => void;
  onRestore: (id: string) => void;
  onSetEnabled: (id: string, enabled: boolean) => void;
  onSetRibbonColor: (id: string, color?: ModeRibbonColor) => void;
  onDelete: (id: string) => void;
  onOpenActions: () => void;
  onChanged?: () => void;
}

export function ProfilesView({
  dataMode,
  profiles,
  actions,
  busy,
  onCreate,
  onParametersForAction,
  onRun,
  onRestore,
  onSetEnabled,
  onSetRibbonColor,
  onDelete,
  onOpenActions,
  onChanged,
}: ProfilesViewProps) {
  const [mode, setMode] = useState<"game" | "manual" | "workspace">("game");
  const [name, setName] = useState("");
  const [exePath, setExePath] = useState("");
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [launchBundle, setLaunchBundle] = useState<"study" | "work" | "creative">("study");
  const [exportedJson, setExportedJson] = useState<string | null>(null);
  const [importText, setImportText] = useState("");
  const [previewItems, setPreviewItems] = useState<readonly ImportPreviewItem[] | null>(null);
  const [ioBusy, setIoBusy] = useState(false);
  const [ioMessage, setIoMessage] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const [ribbonColor, setRibbonColor] = useState<ModeRibbonColor | undefined>();
  const [layoutStatus, setLayoutStatus] = useState<WindowLayoutStatus | null>(null);

  useEffect(() => {
    if (dataMode !== "live") {
      setLayoutStatus(null);
      return;
    }
    let cancelled = false;
    void getWindowLayoutStatus()
      .then((status) => {
        if (!cancelled) setLayoutStatus(status);
      })
      .catch(() => {
        if (!cancelled) setLayoutStatus(null);
      });
    return () => {
      cancelled = true;
    };
  }, [dataMode]);

  /// 実行ファイルは手で打たせない。打ち間違えれば別のファイルが登録される。
  async function chooseExecutable() {
    setPicking(true);
    try {
      const chosen = await pickGameExecutable();
      if (chosen !== null) setExePath(chosen);
    } catch (error: unknown) {
      setIoMessage(publicErrorMessage(error));
    } finally {
      setPicking(false);
    }
  }

  async function doExport() {
    setIoBusy(true);
    setIoMessage(null);
    try {
      setExportedJson(await exportConfig());
    } catch (error: unknown) {
      setIoMessage(publicErrorMessage(error));
    } finally {
      setIoBusy(false);
    }
  }

  async function doPreview() {
    setIoBusy(true);
    setIoMessage(null);
    try {
      setPreviewItems(await importPreview(importText));
    } catch (error: unknown) {
      setPreviewItems(null);
      setIoMessage(publicErrorMessage(error));
    } finally {
      setIoBusy(false);
    }
  }

  async function doApply() {
    setIoBusy(true);
    setIoMessage(null);
    try {
      const result = await importApply(importText);
      setIoMessage(
        `取り込み ${result.imported.length}件` +
          (result.skipped.length > 0 ? ` / スキップ ${result.skipped.length}件` : ""),
      );
      setPreviewItems(null);
      setImportText("");
      onChanged?.();
    } catch (error: unknown) {
      setIoMessage(publicErrorMessage(error));
    } finally {
      setIoBusy(false);
    }
  }

  const live = dataMode === "live";
  const selectable = useMemo(
    () => actions.filter((action) => {
      if (action.id === "setup.window_layout") return false;
      if (action.availability !== "mutable") return false;
      if (mode === "game") {
        return action.autoApplyEligible === true
          && (action.kind === "persistent" || action.kind === "session");
      }
      if (mode === "workspace") {
        return action.kind === "persistent" || action.kind === "session";
      }
      return action.kind === "persistent" || action.kind === "session" || action.kind === "one_way";
    }),
    [actions, mode],
  );

  const canSubmit = live && !busy && name.trim().length > 0
    && (mode === "game"
      ? exePath.trim().length > 0
      : mode === "manual"
        ? selected.size > 0
        : layoutStatus?.saved === true);

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
    const request: CreateProfileRequest = {
      name: name.trim(),
      actions: [
        ...(mode === "workspace"
          ? [{ actionId: "setup.window_layout", parameters: {} }]
          : []),
        ...[...selected].map((actionId) => ({
          actionId,
          parameters: actionId === "setup.launch_apps"
            ? { bundle: launchBundle }
            : onParametersForAction(actionId),
        })),
      ],
      ...(ribbonColor === undefined ? {} : { ribbonColor }),
    };
    if (mode === "game") request.executablePath = exePath.trim();
    onCreate(request);
    setName("");
    setExePath("");
    setSelected(new Set());
    setRibbonColor(undefined);
  }

  return (
    <section className="profiles-view">
      <header className="view-header">
        <span className="eyebrow">利用場面ごとの設定</span>
        <h1>モード</h1>
        {/* 以前は95文字の一文だった。読点でつなぐと、どこまでが1つの話か分からなくなる。 */}
        <p>
          ゲームは、起動と終了に合わせて自動で切り替えられます。
          勉強や作業は手動にして、<strong>「いま実行」を押したときだけ</strong>まとめて適用します。
          適用したものは、あとから1件ずつ元へ戻せます。
        </p>
      </header>

      {live ? null : (
        <div className="inline-note" role="note">
          <Icon name="info" />
          <span>閲覧モードです。安全コアに接続すると、モードの作成と有効化ができます。</span>
        </div>
      )}

      <div className="inline-note" role="note">
        <Icon name="undo" />
        <span>
          <strong>一時ワークスペース</strong>は、保存済みの窓配置と選んだ設定をまとめて適用します。
          「終わる」は窓と設定を戻します。アプリは閉じません。
          戻すのは、このアプリが動かした分だけです。
        </span>
      </div>

      <div className={`profiles-layout${profiles.length === 0 ? " profiles-layout--empty" : ""}`}>
        <form
          className="profile-create"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <h2>新しいモード</h2>
          {/* Apple Shortcuts の「〜のとき、〜する」。入力するとそのまま文章になり、
              専門用語を読まなくても何が起きるか分かるようにする。 */}
          <fieldset className="field">
            <legend>モードの動かし方</legend>
            <div className="action-picker">
              <label className="action-pick">
                <input checked={mode === "game"} disabled={!live || busy} name="profile-mode" onChange={() => { setMode("game"); setSelected(new Set()); }} type="radio" />
                <span><strong>ゲーム（自動）</strong><small>登録したゲームの起動で適用し、終了で元に戻します。</small></span>
              </label>
              <label className="action-pick">
                <input checked={mode === "manual"} disabled={!live || busy} name="profile-mode" onChange={() => { setMode("manual"); setSelected(new Set()); }} type="radio" />
                <span><strong>勉強・作業など（手動）</strong><small>実行ファイルとは紐付けず、「いま実行」のときだけ適用します。</small></span>
              </label>
              <label className="action-pick">
                <input checked={mode === "workspace"} disabled={!live || busy} name="profile-mode" onChange={() => { setMode("workspace"); setSelected(new Set()); }} type="radio" />
                <span><strong>一時ワークスペース</strong><small>保存済みの窓配置へ切り替え、終わると開始直前の窓と選んだ設定だけを戻します。</small></span>
              </label>
            </div>
          </fieldset>
          <p className="automation-sentence">
            {mode === "game" ? <>
              <strong>{name.trim() || "このゲーム"}</strong> が始まったら、
              {selected.size === 0 ? "選んだ準備" : <strong>{selected.size}件の準備</strong>}
              をして、終わったら<strong>変更した分だけ元に戻します</strong>。
            </> : mode === "workspace" ? <>
              <strong>{name.trim() || "このワークスペース"}</strong>で「作業を始める」と、
              保存済みの窓配置
              {selected.size === 0 ? null : <>と<strong>{selected.size}件の設定</strong></>}
              を適用します。「終わる」と<strong>このアプリが動かした分だけ</strong>戻します。
            </> : <>
              <strong>{name.trim() || "このモード"}</strong>を「いま実行」したら、
              {selected.size === 0 ? "選んだ準備" : <strong>{selected.size}件の準備</strong>}
              をします。アプリ起動は<strong>元に戻しても終了しません</strong>。
            </>}
          </p>
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
          <fieldset className="field">
            <legend>モードリボン（任意）</legend>
            <p className="muted small">
              このモードの実行中だけ、プライマリタスクバーのすぐ上へ4pxの帯を出します。
              全画面・自動的に隠す設定・上下左右のうち下以外では表示しません。
            </p>
            <RibbonColorPicker
              disabled={!live || busy}
              name="new-profile-ribbon-color"
              onChange={setRibbonColor}
              value={ribbonColor}
            />
          </fieldset>
          {mode === "game" ? (
            <label className="field">
              <span>ゲームの実行ファイル</span>
              <div className="file-pick">
                <input
                  disabled={!live || busy}
                  onChange={(event) => setExePath(event.target.value)}
                  placeholder="「選ぶ」から探せます"
                  spellCheck={false}
                  type="text"
                  value={exePath}
                />
                <button
                  className="secondary-button"
                  disabled={!live || busy || picking}
                  onClick={() => void chooseExecutable()}
                  type="button"
                >
                  {picking ? <Icon className="spin" name="spinner" /> : <Icon name="explorer" />}選ぶ
                </button>
              </div>
              <small>このPC上に実在する実行ファイルだけを登録できます。別のファイルに差し替わっていないか、起動のたびに確かめます。</small>
            </label>
          ) : null}

          <fieldset className="field">
            <legend>{mode === "game" ? "起動時にまとめる準備" : mode === "workspace" ? "窓配置と一緒に使う設定（任意）" : "いま実行する準備"}</legend>
            {mode === "workspace" ? (
              <p className={layoutStatus?.saved === true ? "muted small" : "inline-error"} role={layoutStatus?.saved === true ? undefined : "alert"}>
                {layoutStatus?.saved === true
                  ? `セットアップで保存した${layoutStatus.windowCount}個の窓だけを対象にします。開始後に開いた窓は戻しません。`
                  : "先にセットアップ画面で現在のウィンドウ配置を保存してください。保存されるまで作成できません。"}
              </p>
            ) : null}
            {selectable.length === 0 && dataMode !== "live" ? (
              <p className="muted">安全コアへ接続していないため、選べる項目を読み取れていません。</p>
            ) : selectable.length === 0 ? (
              <p className="muted">
                選べる項目がありません。<button className="link-button" onClick={onOpenActions} type="button">変更できる項目</button>を確認してください。
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
              {mode === "manual" && selected.has("setup.launch_apps") ? (
                <label className="field">
                  <span>まとめて開くアプリ</span>
                  <select disabled={!live || busy} onChange={(event) => setLaunchBundle(event.target.value as "study" | "work" | "creative")} value={launchBundle}>
                    <option value="study">勉強: Microsoft Edge＋メモ帳</option>
                    <option value="work">作業: Microsoft Edge＋メモ帳＋電卓</option>
                    <option value="creative">制作: ペイント＋メモ帳</option>
                  </select>
                  <small>コード内固定リストのみ。任意のパス・アプリID・引数は受け付けません。</small>
                </label>
              ) : null}
          </fieldset>

          <button className="primary-button" disabled={!canSubmit} type="submit">
            {busy ? <Icon className="spin" name="spinner" /> : <Icon name="plus" />}
            {mode === "workspace" ? "一時ワークスペースを作成" : "モードを作成"}
          </button>
          <p className="muted small">
            {mode === "game"
              ? "作成しても自動適用はまだ開始しません。各モードの「自動適用」を有効にしたときだけ働きます。"
              : mode === "workspace"
                ? "作成しただけでは変更しません。「作業を始める」で初めてpreview→commitを通して適用します。"
                : "手動モードは自動適用されません。カードの「いま実行」を押したときだけ働きます。"}
          </p>
        </form>

        <div className="profile-list-panel">
          <h2>登録済みのモード</h2>
          {profiles.length === 0 && dataMode !== "live" ? (
            /* 0件を「無い」と言い切らない。
               安全コアへ繋がっていないときは、そもそも読めていない。
               読めなかったことを「まだありません」と書くと、
               登録済みのモードが消えたように見える。 */
            <div className="empty-block">
              <Icon name="warning" />
              <strong>登録済みのモードを読み取れていません</strong>
              <span>
                {dataMode === "loading"
                  ? "読み込み中です。"
                  : "安全コアへ接続できていないため、この一覧は空のままです。登録したモードが消えたわけではありません。"}
              </span>
            </div>
          ) : profiles.length === 0 ? (
            <div className="empty-block">
              <Icon name="game" />
              <strong>まだモードはありません</strong>
              <span>左のフォームから、ゲーム・勉強・作業などのモードを登録できます。</span>
            </div>
          ) : (
            <ul className="profile-list">
              {profiles.map((profile) => {
                const workspace = profile.executablePath === undefined
                  && profile.actions.some((action) => action.actionId === "setup.window_layout");
                return (
                <li className="profile-card" key={profile.id}>
                  <div className="profile-card__head">
                    <div>
                      <strong>{profile.name}</strong>
                      <code title={profile.executablePath}>{profile.executablePath === undefined ? "実行ファイルなし（手動）" : fileName(profile.executablePath)}</code>
                    </div>
                    <span
                      className={`profile-state profile-state--${
                        profile.executablePath === undefined
                          ? (profile.activeRun === undefined ? "off" : "on")
                          : (profile.automationEnabled ? "on" : "off")
                      }`}
                    >
                      {profile.executablePath === undefined
                        ? workspace
                          ? (profile.activeRun === undefined ? "作業前" : "作業中")
                          : (profile.activeRun === undefined ? "手動・待機中" : "手動・実行中")
                        : (profile.automationEnabled ? "自動適用オン" : "自動適用オフ")}
                    </span>
                  </div>
                  <div className="profile-card__actions-count">
                    <Icon name="action" size={15} />
                    <span>{profile.actions.length}件の準備</span>
                  </div>
                  <div className="profile-card__ribbon">
                    <span>
                      <strong>モードリボン</strong>
                      <small>実行中だけ表示。クリックは下へ通ります。</small>
                    </span>
                    <RibbonColorPicker
                      disabled={!live || busy}
                      name={`profile-ribbon-${profile.id}`}
                      onChange={(color) => onSetRibbonColor(profile.id, color)}
                      value={profile.ribbonColor}
                    />
                  </div>
                  <div className="profile-card__controls">
                    {profile.executablePath === undefined ? (
                      <button
                        className="secondary-button"
                        disabled={!live || busy}
                        onClick={() => profile.activeRun === undefined ? onRun(profile.id) : onRestore(profile.id)}
                        type="button"
                      >
                        {workspace
                          ? (profile.activeRun === undefined ? "作業を始める" : "終わる")
                          : (profile.activeRun === undefined ? "いま実行" : "実行した分を戻す")}
                      </button>
                    ) : (
                      <button
                        className="secondary-button"
                        disabled={!live || busy}
                        onClick={() => onSetEnabled(profile.id, !profile.automationEnabled)}
                        type="button"
                      >
                        {profile.automationEnabled ? "自動適用を止める" : "自動適用を有効にする"}
                      </button>
                    )}
                    <button
                      aria-label={`${profile.name}を削除`}
                      className="icon-button"
                      disabled={!live || busy || profile.activeRun !== undefined}
                      onClick={() => onDelete(profile.id)}
                      type="button"
                    >
                      <Icon name="close" />
                    </button>
                  </div>
                </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>

      {/* モード画面の主目的はモードの作成と一覧。パネルはその下。
          上に置いていたとき、作成フォームは 618px の位置から始まっていた（画面は 720px）。 */}
      <HotCornerPanel dataMode={dataMode} />
      <ShareSessionPanel dataMode={dataMode} />

      <GameReadinessPanel dataMode={dataMode} />

      <div className="config-io">
        <h2>設定のバックアップ・移行</h2>
        <p className="muted small">
          登録したモード定義だけをJSONとして書き出し・取り込みます。任意コードやスクリプトは含みません。
          ゲーム用は別PCで実行ファイルを再確認します。手動モードは実行ファイルなしのまま取り込みます。
        </p>
        <div className="config-io__row">
          <button className="secondary-button" disabled={!live || ioBusy} onClick={() => void doExport()} type="button">
            <Icon name="arrow" />エクスポート
          </button>
          {exportedJson === null ? null : (
            <textarea
              aria-label="エクスポートしたバックアップJSON"
              className="config-io__text"
              readOnly
              rows={4}
              value={exportedJson}
            />
          )}
        </div>
        <div className="config-io__row">
          <textarea
            aria-label="取り込むバックアップJSON"
            className="config-io__text"
            disabled={!live || ioBusy}
            onChange={(event) => setImportText(event.target.value)}
            placeholder="ここにバックアップJSONを貼り付けて『内容を確認』"
            rows={4}
            value={importText}
          />
          <div className="config-io__actions">
            <button
              className="secondary-button"
              disabled={!live || ioBusy || importText.trim().length === 0}
              onClick={() => void doPreview()}
              type="button"
            >
              内容を確認
            </button>
            <button
              className="primary-button"
              disabled={!live || ioBusy || previewItems === null}
              onClick={() => void doApply()}
              type="button"
            >
              {ioBusy ? <Icon className="spin" name="spinner" /> : <Icon name="check" />}取り込む
            </button>
          </div>
        </div>
        {previewItems === null ? null : (
          <ul className="config-io__preview">
            {previewItems.map((item) => (
              <li key={`${item.name}-${item.executablePath}`} className={item.resolvable ? "" : "config-io__preview--blocked"}>
                <Icon name={item.resolvable ? "check" : "warning"} size={15} />
                <span><strong>{item.name}</strong> — {item.note}</span>
              </li>
            ))}
          </ul>
        )}
        {ioMessage === null ? null : <p className="config-io__message" role="status">{ioMessage}</p>}
      </div>
    </section>
  );
}

function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

interface RibbonColorChoice {
  value?: ModeRibbonColor;
  label: string;
}

const RIBBON_COLOR_CHOICES: readonly RibbonColorChoice[] = [
  { label: "表示しない" },
  { value: "sky", label: "空色" },
  { value: "violet", label: "紫" },
  { value: "mint", label: "ミント" },
  { value: "amber", label: "琥珀" },
  { value: "rose", label: "ローズ" },
];

interface RibbonColorPickerProps {
  disabled: boolean;
  name: string;
  value: ModeRibbonColor | undefined;
  onChange: (color?: ModeRibbonColor) => void;
}

function RibbonColorPicker({ disabled, name, value, onChange }: RibbonColorPickerProps) {
  return (
    <div className="ribbon-color-picker">
      {RIBBON_COLOR_CHOICES.map((choice) => (
        <label
          className={`ribbon-color-option ribbon-color-option--${choice.value ?? "off"}`}
          key={choice.value ?? "off"}
          title={choice.label}
        >
          <input
            aria-label={choice.label}
            checked={value === choice.value}
            disabled={disabled}
            name={name}
            onChange={() => onChange(choice.value)}
            type="radio"
          />
          <span aria-hidden="true" />
        </label>
      ))}
    </div>
  );
}
