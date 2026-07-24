fn main() {
    #[cfg(target_os = "windows")]
    {
        let attributes = tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new()
                .app_manifest(include_str!("windows-app-manifest.xml")),
        );
        tauri_build::try_build(attributes).expect("Tauri build metadata generation failed");
    }

    #[cfg(not(target_os = "windows"))]
    tauri_build::build();
}

