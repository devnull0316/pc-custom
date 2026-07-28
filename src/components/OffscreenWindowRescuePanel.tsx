import { useEffect, useId, useState } from "react";

import {
  listOffscreenWindows,
  publicErrorMessage,
  rescueOffscreenWindow,
  rollbackOffscreenWindow,
} from "../backend";
import type {
  BootstrapStatus,
  DataMode,
  OffscreenWindowBlockReason,
  OffscreenWindowCandidate,
  OffscreenWindowScan,
  OffscreenWindowUndo,
} from "../model";
import { Dialog } from "./Dialog";
import { Icon } from "./Icon";

interface OffscreenWindowRescuePanelProps {
  bootstrap: BootstrapStatus | null;
  dataMode: DataMode;
  onError: (error: unknown) => void;
  onNotice: (message: string) => void;
}

type ScanState =
  | { phase: "idle" }
  | { phase: "loading" }
  | { phase: "loaded"; value: OffscreenWindowScan }
  | { phase: "error"; message: string }
  | { phase: "unavailable" };

type PendingOperation =
  | { kind: "rescue"; id: string }
  | { kind: "rollback"; id: string };

function blockReasonLabel(reason: OffscreenWindowBlockReason | null): string {
  switch (reason) {
    case "higher_integrity":
      return "管理者として動いているため、この画面からは戻せません。";
    case "not_responding":
      return "アプリから応答がないため、安全に動かせません。";
    case "access_unknown":
      return "同じウィンドウか安全に確認できないため、動かせません。";
    case "already_rescued":
      return "すでにPCカスタムで救出済みです。下のボタンから元の位置へ戻せます。";
    case null:
      return "安全に操作できることを確認できないため、動かせません。";
  }
}

function scanStateLabel(state: ScanState): string {
  switch (state.phase) {
    case "idle":
      return "まだ確認していません";
    case "loading":
      return "見失ったウィンドウを確認中";
    case "error":
      return "現在の状態を確認できませんでした";
    case "unavailable":
      return "安全コアに接続すると確認できます";
    case "loaded": {
      const rescuable = state.value.candidates.filter((candidate) => candidate.canRescue).length;
      const blocked = state.value.candidates.length - rescuable;
      if (state.value.candidates.length === 0) {
        return state.value.undoItems.length === 0
          ? "見失った対象ウィンドウはありません"
          : `見失った対象はありません・元へ戻せる移動 ${state.value.undoItems.length}件`;
      }
      return `救出できるウィンドウ ${rescuable}件` +
        (blocked > 0 ? `・動かせないウィンドウ ${blocked}件` : "");
    }
  }
}

function addUndoItem(
  scan: OffscreenWindowScan,
  undo: OffscreenWindowUndo,
  candidateId: string,
): OffscreenWindowScan {
  return {
    ...scan,
    candidates: scan.candidates.map((candidate) =>
      candidate.candidateId === candidateId
        ? {
            ...candidate,
            canRescue: false,
            unavailableReason: "already_rescued",
          }
        : candidate,
    ),
    undoItems: [
      ...scan.undoItems.filter((item) => item.undoId !== undo.undoId),
      undo,
    ],
  };
}

export function OffscreenWindowRescuePanel({
  bootstrap,
  dataMode,
  onError,
  onNotice,
}: OffscreenWindowRescuePanelProps) {
  const live = dataMode === "live";
  const mutationAllowed = live && bootstrap?.mode === "ready";
  const radioGroupName = useId();
  const [scanState, setScanState] = useState<ScanState>({ phase: "idle" });
  const [selectedCandidateId, setSelectedCandidateId] = useState<string | null>(null);
  const [confirmingCandidateId, setConfirmingCandidateId] = useState<string | null>(null);
  const [pending, setPending] = useState<PendingOperation | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (dataMode === "loading") {
      setScanState({ phase: "idle" });
      return;
    }
    if (!live) {
      setScanState({ phase: "unavailable" });
      return;
    }

    let cancelled = false;
    setScanState({ phase: "loading" });
    void listOffscreenWindows()
      .then((scan) => {
        if (!cancelled) setScanState({ phase: "loaded", value: scan });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        const errorMessage = publicErrorMessage(error);
        setScanState({ phase: "error", message: errorMessage });
        onError(error);
      });
    return () => {
      cancelled = true;
    };
  }, [dataMode, live, onError]);

  async function refreshScan() {
    if (!live || pending !== null || scanState.phase === "loading") return;
    setSelectedCandidateId(null);
    setConfirmingCandidateId(null);
    setMessage(null);
    setScanState({ phase: "loading" });
    try {
      setScanState({ phase: "loaded", value: await listOffscreenWindows() });
    } catch (error: unknown) {
      const errorMessage = publicErrorMessage(error);
      setScanState({ phase: "error", message: errorMessage });
      onError(error);
    }
  }

  async function rescue(candidate: OffscreenWindowCandidate) {
    if (!mutationAllowed || !candidate.canRescue || pending !== null) return;
    setPending({ kind: "rescue", id: candidate.candidateId });
    setMessage(null);
    try {
      const undo = await rescueOffscreenWindow(candidate.candidateId);
      setScanState((current) =>
        current.phase === "loaded"
          ? {
              phase: "loaded",
              value: addUndoItem(current.value, undo, candidate.candidateId),
            }
          : current,
      );
      setSelectedCandidateId(null);
      setConfirmingCandidateId(null);
      setMessage(`${undo.applicationLabel}を今の画面へ戻しました。`);
      onNotice(`${undo.applicationLabel}を今の画面へ戻しました。`);
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
      onError(error);
    } finally {
      setPending(null);
    }
  }

  async function rollback(undo: OffscreenWindowUndo) {
    if (!live || pending !== null) return;
    setPending({ kind: "rollback", id: undo.undoId });
    setMessage(null);
    try {
      const restored = await rollbackOffscreenWindow(undo.undoId);
      let refreshed: OffscreenWindowScan | null = null;
      try {
        refreshed = await listOffscreenWindows();
      } catch {
        // 復元自体は成功している。古い候補IDを再利用せず、残るundoだけを保持する。
      }
      setScanState((current) => {
        if (refreshed !== null) return { phase: "loaded", value: refreshed };
        if (current.phase !== "loaded") return current;
        return {
          phase: "loaded",
          value: {
            ...current.value,
            candidates: [],
            undoItems: current.value.undoItems.filter((item) => item.undoId !== undo.undoId),
          },
        };
      });
      setSelectedCandidateId(null);
      setConfirmingCandidateId(null);
      setMessage(
        refreshed === null
          ? `${restored.applicationLabel}を元の位置へ戻しました。一覧はもう一度確認してください。`
          : `${restored.applicationLabel}を元の位置へ戻しました。`,
      );
      onNotice(`${restored.applicationLabel}を元の位置へ戻しました。`);
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
      onError(error);
    } finally {
      setPending(null);
    }
  }

  const scan = scanState.phase === "loaded" ? scanState.value : null;
  const selectedCandidate =
    scan?.candidates.find(
      (candidate) =>
        candidate.candidateId === selectedCandidateId && candidate.canRescue,
    ) ?? null;
  const confirmingCandidate =
    scan?.candidates.find(
      (candidate) =>
        candidate.candidateId === confirmingCandidateId && candidate.canRescue,
    ) ?? null;
  const scanBusy = scanState.phase === "loading";

  return (
    <>
      <section
        aria-labelledby="offscreen-window-rescue-title"
        className="setup-essentials offscreen-window-rescue"
      >
        <header className="setup-essentials__header">
          <span className="eyebrow">ウィンドウの救出</span>
          <h2 id="offscreen-window-rescue-title">見失ったウィンドウを今の画面へ戻す</h2>
          <p>
            接続中のどの画面にも十分見えていないウィンドウを探し、
            <strong>選んだ1つだけ</strong>を今の画面へ移動します。
            保存済みのウィンドウ配置は使いません。
          </p>
        </header>

        <article className="setup-essential-card setup-essential-card--wide">
          <div className="setup-essential-card__copy">
            <span className="setup-essential-card__icon"><Icon name="recovery" /></span>
            <div>
              <h3>画面の外に残ったアプリを1つずつ戻す</h3>
              <p>
                モニターやドックを外した後などに、開いているのに見えなくなったアプリを探します。
                アプリ名は実行ファイル名から表示し、ウィンドウのタイトルは画面に表示しません。
              </p>
            </div>
          </div>

          <dl
            aria-label="見失ったウィンドウを戻す機能の対象・状態・方法"
            className="setup-essential-card__contract"
          >
            <div>
              <dt>どんな人向け</dt>
              <dd>モニターやドックを外した後、開いているアプリが見えなくなった人向け</dd>
            </div>
            <div>
              <dt>現在の状態</dt>
              <dd>{scanStateLabel(scanState)}</dd>
            </div>
            <div>
              <dt>適用後の状態</dt>
              <dd>選んだ1つのウィンドウが、今の画面の作業領域内に入ります</dd>
            </div>
            <div>
              <dt>変更方法</dt>
              <dd>Windowsが公開しているウィンドウ配置の機能で、同じウィンドウかを毎回確認します</dd>
            </div>
          </dl>
          <ul aria-label="見失ったウィンドウを戻す機能の属性" className="setup-essential-card__metadata">
            <li>危険度: 注意</li>
            <li>管理者: 不要</li>
            <li>再起動: 不要</li>
            <li>Update影響: 低い</li>
            <li>復元: 1件ずつ元に戻せる</li>
          </ul>

          <div className="window-layout-boundary" role="note">
            <Icon name="info" size={16} />
            <p>
              管理者として動くアプリや応答のないアプリは勝手に動かさず、一覧に理由を表示します。
              登録済みゲーム、全画面表示、Windowsの画面部品は対象外です。
              元へ戻すときも、同じウィンドウでなくなっていた場合は操作しません。
            </p>
          </div>

          <div className="setup-essential-card__actions">
            <button
              className="secondary-button"
              disabled={!live || scanBusy || pending !== null}
              onClick={() => void refreshScan()}
              type="button"
            >
              {scanBusy
                ? <Icon className="spin" name="spinner" />
                : <Icon name="search" />}
              {scan === null ? "見失ったウィンドウを探す" : "もう一度探す"}
            </button>
            <button
              className="primary-button"
              disabled={
                !mutationAllowed ||
                selectedCandidate === null ||
                scanBusy ||
                pending !== null
              }
              onClick={() => {
                if (selectedCandidate !== null) {
                  setConfirmingCandidateId(selectedCandidate.candidateId);
                }
              }}
              type="button"
            >
              <Icon name="recovery" />
              選んだ1つを確認
            </button>
          </div>

          {scanState.phase === "error" ? (
            <p className="setup-essential-card__message" role="status">{scanState.message}</p>
          ) : null}
          {!live ? (
            <p className="muted small">閲覧モードです。安全コアに接続すると確認できます。</p>
          ) : !mutationAllowed ? (
            <p className="muted small">
              現在は変更操作を停止しています。安全コアが準備できると、選んだ1つを戻せます。
            </p>
          ) : null}

          {scan === null || scan.candidates.length === 0 ? (
            scanState.phase === "loaded" ? (
              <p className="muted small">今の画面から外れた対象ウィンドウは見つかりませんでした。</p>
            ) : null
          ) : (
            <fieldset className="offscreen-window-rescue__fieldset">
              <legend>戻すウィンドウを1つ選ぶ</legend>
              <ul className="offscreen-window-rescue__list">
                {scan.candidates.map((candidate) => {
                  const descriptionId = `offscreen-window-${candidate.candidateId}`;
                  const selectable = candidate.canRescue && mutationAllowed && pending === null;
                  return (
                    <li key={candidate.candidateId}>
                      <label
                        className={`offscreen-window-rescue__candidate${
                          candidate.canRescue
                            ? ""
                            : " offscreen-window-rescue__candidate--blocked"
                        }`}
                      >
                        <input
                          aria-describedby={descriptionId}
                          checked={selectedCandidateId === candidate.candidateId}
                          disabled={!selectable}
                          name={radioGroupName}
                          onChange={() => setSelectedCandidateId(candidate.candidateId)}
                          type="radio"
                          value={candidate.candidateId}
                        />
                        <span>
                          <strong>{candidate.applicationLabel}</strong>
                          <small id={descriptionId}>
                            {candidate.canRescue
                              ? "接続中のどの画面にも十分表示されていません。"
                              : blockReasonLabel(candidate.unavailableReason)}
                          </small>
                        </span>
                      </label>
                    </li>
                  );
                })}
              </ul>
            </fieldset>
          )}

          {scan === null || scan.excludedGameWindows + scan.skippedWindows === 0 ? null : (
            <p className="muted small">
              登録済みゲーム {scan.excludedGameWindows}件と、安全に対象確認できない
              {scan.skippedWindows}件は操作対象から除外しました。
            </p>
          )}

          {scan === null || scan.undoItems.length === 0 ? null : (
            <section
              aria-labelledby="offscreen-window-undo-title"
              className="offscreen-window-rescue__undo"
            >
              <div>
                <h4 id="offscreen-window-undo-title">元の位置へ戻せる移動</h4>
                <p>PCカスタムが移動した1件ごとに、直前の位置へ戻せます。</p>
              </div>
              <ul>
                {scan.undoItems.map((undo) => {
                  const rollingBack =
                    pending?.kind === "rollback" && pending.id === undo.undoId;
                  return (
                    <li key={undo.undoId}>
                      <strong>{undo.applicationLabel}</strong>
                      <button
                        className="secondary-button"
                        disabled={!live || pending !== null}
                        onClick={() => void rollback(undo)}
                        type="button"
                      >
                        {rollingBack
                          ? <Icon className="spin" name="spinner" />
                          : <Icon name="undo" />}
                        元の位置へ戻す
                      </button>
                    </li>
                  );
                })}
              </ul>
            </section>
          )}

          {message === null ? null : (
            <p className="setup-essential-card__message" role="status">{message}</p>
          )}
        </article>
      </section>

      {confirmingCandidate === null ? null : (
        <Dialog
          description="保存済みの配置は使わず、選んだ1つだけを移動します。"
          footer={(
            <>
              <button
                className="secondary-button"
                data-dialog-autofocus
                disabled={pending !== null}
                onClick={() => setConfirmingCandidateId(null)}
                type="button"
              >
                キャンセル
              </button>
              <button
                className="primary-button"
                disabled={pending !== null}
                onClick={() => void rescue(confirmingCandidate)}
                type="button"
              >
                {pending?.kind === "rescue"
                  ? <Icon className="spin" name="spinner" />
                  : <Icon name="recovery" />}
                この1つを今の画面へ戻す
              </button>
            </>
          )}
          onClose={() => {
            if (pending === null) setConfirmingCandidateId(null);
          }}
          title="このウィンドウを今の画面へ戻しますか？"
        >
          <div className="rollback-summary">
            <strong>{confirmingCandidate.applicationLabel}</strong>
            <div>
              <span>
                <small>現在</small>
                接続中のどの画面にも十分表示されていません
              </span>
              <Icon name="arrow" />
              <span>
                <small>救出後</small>
                今の画面の作業領域内
              </span>
            </div>
            <p>
              同じウィンドウかを移動直前にも確認します。救出後は、この画面から元の位置へ戻せます。
            </p>
          </div>
        </Dialog>
      )}
    </>
  );
}
