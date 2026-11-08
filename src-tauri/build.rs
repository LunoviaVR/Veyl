fn main() {
    // Release builds start elevated (UAC prompt) so frame capture, kernel dumps and
    // protected-process details work without a restart. Debug builds stay asInvoker:
    // an exe that requires elevation cannot be launched by `tauri dev` (OS error 740).
    println!("cargo:rerun-if-changed=manifests/admin.xml");
    let release = std::env::var("PROFILE").is_ok_and(|p| p == "release");
    let mut windows = tauri_build::WindowsAttributes::new();
    if release {
        windows = windows.app_manifest(include_str!("manifests/admin.xml"));
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri build script failed");
}
