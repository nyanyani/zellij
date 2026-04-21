use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-changed=zellij-utils/assets/libs/conpty.dll");

    let target_is_windows = env::var("CARGO_CFG_TARGET_OS").ok().as_deref() == Some("windows");
    let profile = env::var("PROFILE").unwrap_or_default();

    // The clap-derived `augment_subcommands` for `CliAction` (~70 variants)
    // produces a >1 MB stack frame in debug mode, overflowing the Windows
    // default 1 MB main-thread stack. Increase it to 8 MB to match Linux.
    // Release builds optimize the frame down, so this is only needed for non-release profiles.
    if target_is_windows && profile != "release" {
        println!("cargo:rustc-link-arg=/STACK:8388608");
    }

    // Embed the application icon into the Windows executable.
    #[cfg(target_os = "windows")]
    let _ = embed_resource::compile("assets/zellij.rc", embed_resource::NONE);

    if !target_is_windows {
        return;
    }

    let lib_path = get_output_path();
    let conpty_source = Path::new("zellij-utils")
        .join("assets")
        .join("libs")
        .join("conpty.dll");
    let conpty_destination = lib_path.join("conpty.dll");

    fs::copy(conpty_source, conpty_destination).unwrap();
}

fn get_output_path() -> PathBuf {
    // <target-dir>/<triple?>/<profile>/build/<pkg>/out -> ascend to <target-dir>/<triple?>/<profile>/
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR should contain a Cargo profile directory")
        .to_path_buf()
}
