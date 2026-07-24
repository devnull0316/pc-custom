fn main() {
    // 以前のカスタム windows-app-manifest.xml は <dpiAwareness>/<longPathAware> の要素値に
    // 空白・改行を含んでおり、Windows ローダがプロセス生成時に fastfail(0xC0000409, main到達前)
    // する原因だった。tauri 既定マニフェスト(asInvoker + PerMonitorV2 を含む)を使う。
    tauri_build::build();
}

