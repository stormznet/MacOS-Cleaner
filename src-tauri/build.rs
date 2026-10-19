fn main() {
    cc::Build::new()
        .file("native/macos.m")
        .flag("-fobjc-arc")
        .flag("-mmacosx-version-min=13.0")
        .compile("mac_cleaner_native");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rerun-if-changed=native/macos.m");
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "bootstrap",
            "scan",
            "cancel_scan",
            "choose_root",
            "remove_root",
            "set_appearance",
            "trash_items",
            "restore_item",
            "reveal_item",
            "open_trash",
            "open_privacy_settings",
        ]),
    ))
    .expect("could not build Tauri configuration");
}
