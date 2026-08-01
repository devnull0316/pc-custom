import type { ActionPresentation, RiskLevel } from "./model";

const INTERNAL_IMPLEMENTATION = /(?:HKCU|HKLM|HKEY|GUID|API|Core Audio|EndpointVolume|eCommunications|registry|レジストリ|DWORD|PowerGet|PowerSet|DwmGet|GetWindows|GetDisk|GetTemp|SystemParametersInfo|Known Folder|\\)/i;

export function screenText(value: string, fallback: string): string {
  return INTERNAL_IMPLEMENTATION.test(value) ? fallback : value;
}

export function methodSummaryForScreen(action: ActionPresentation): string {
  if (action.kind === "guided") {
    return "PCカスタムは変更せず、Windowsの設定画面で確認できる場所を案内します。";
  }
  if (action.methodClass === "winget") {
    return "Windowsの標準のアプリ導入機能を使います。";
  }
  if (action.kind === "observation" || action.availability === "read_only") {
    return "Windowsから、この項目に必要な範囲だけを読み取ります。";
  }
  return action.reversible
    ? "Windowsのこの項目だけを変更し、適用前の状態を保存して元へ戻せるようにします。"
    : "Windowsのこの項目に必要な範囲だけを変更します。";
}

export function detailPointsForScreen(action: ActionPresentation): readonly string[] {
  const fallback = action.reversible
    ? "適用前の状態を保存し、この項目だけを元へ戻せます。"
    : "Windowsから、この項目に必要な範囲だけを扱います。";
  return [...new Set(action.detailPoints.map((point) => screenText(point, fallback)))];
}

export function getRiskReasons(action: {
  riskLevel: RiskLevel;
  requiresAdmin: boolean;
  requiresRestart: boolean;
  requiresExplorerRestart: boolean;
  updateImpact: "low" | "review" | "high";
  reversible: boolean;
}): string[] {
  const reasons: string[] = [];
  if (!action.reversible) {
    reasons.push("変更前の状態への自動復元に対応していません（元に戻せません）。");
  }
  if (action.requiresAdmin) {
    reasons.push("設定の変更時に管理者権限（UAC確認）が必要です。");
  }
  if (action.requiresRestart) {
    reasons.push("反映には Windows の再起動が必要です。");
  }
  if (action.requiresExplorerRestart) {
    reasons.push("反映にはエクスプローラーの再起動が必要です。");
  }
  if (action.updateImpact === "review") {
    reasons.push("Windows Update の後に再検証が必要です。");
  } else if (action.updateImpact === "high") {
    reasons.push("Windows Update の影響を受けやすい項目です。");
  }
  if (action.riskLevel === "experimental") {
    reasons.push("一部の環境で意図しない挙動になる可能性がある実験的な項目です。");
  }
  return reasons;
}

