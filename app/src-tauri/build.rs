use std::path::PathBuf;

/// Common-Controls v6 application manifest (same content tauri-build embeds by
/// default). Tauri links comctl32 v6-only APIs (e.g. `TaskDialogIndirect`), so
/// every Windows executable that links tauri must carry this manifest or the
/// loader aborts it with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139).
const WINDOWS_MANIFEST_FILE: &str = "windows-app-manifest.xml";

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let mut attributes = tauri_build::Attributes::new();
    if target_os == "windows" && target_env == "msvc" {
        // tauri-build only embeds its manifest into `[[bin]]` targets (via the
        // resource file), leaving integration-test executables without it. Embed
        // the manifest through the linker instead, for both bins and tests, and
        // stop tauri-build from adding a second (conflicting) manifest resource.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        embed_windows_manifest();
    }
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}

fn embed_windows_manifest() {
    let manifest =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(WINDOWS_MANIFEST_FILE);
    println!("cargo:rerun-if-changed={}", manifest.display());
    for kind in ["bins", "tests"] {
        println!("cargo:rustc-link-arg-{kind}=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-{kind}=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
