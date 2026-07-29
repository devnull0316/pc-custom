import type { ActionPresentation } from "./model";

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
