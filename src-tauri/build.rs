fn main() {
    seed_index();
    // tauri-build embeds icons/icon.ico in the Windows binary but only asks
    // to rerun when tauri.conf.json or the capabilities change, so a new icon
    // would never reach an existing build.
    println!("cargo:rerun-if-changed=icons");
    // tauri-build embeds the Common Controls v6 manifest only into the app
    // binary, but unit-test binaries need it too, otherwise they fail to start
    // on Windows with STATUS_ENTRYPOINT_NOT_FOUND (TaskDialogIndirect). So we
    // let the linker embed it for every target instead.
    let mut windows = tauri_build::WindowsAttributes::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}

/// Embeds the published mod index that `release.yml` puts into `seed/`
/// (`mod_index::SEED`); without it, empty files, so the app starts with no
/// index until its first check, as before.
fn seed_index() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set"));
    for name in ["index.json", "index.json.minisig"] {
        let src = std::path::Path::new("seed").join(name);
        println!("cargo:rerun-if-changed={}", src.display());
        let data = std::fs::read(&src).unwrap_or_default();
        std::fs::write(out.join(format!("seed-{name}")), data).expect("OUT_DIR is writable");
    }
}
