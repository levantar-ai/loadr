//! Shared helpers: build the example plugins with the real cargo and locate
//! their artifacts. Build failures fail the calling test loudly.

// Each test binary uses a subset of these helpers.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

pub fn workspace_root() -> PathBuf {
    // crates/loadr-plugin-api -> workspace root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root exists")
        .to_path_buf()
}

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

fn run_build(mut cmd: Command, what: &str) {
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("cannot run cargo for {what}: {e}"));
    assert!(
        output.status.success(),
        "building {what} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Build a standalone wasm guest crate for wasm32-wasip2 and return the path
/// to the produced component.
pub fn build_wasm_guest(dir_name: &str, artifact: &str) -> PathBuf {
    let guest_dir = workspace_root().join("plugins/examples").join(dir_name);
    let mut cmd = cargo();
    cmd.args(["build", "--release", "--target", "wasm32-wasip2"])
        .current_dir(&guest_dir)
        // The guest is its own workspace with its own target dir.
        .env_remove("CARGO_TARGET_DIR");
    run_build(cmd, dir_name);
    let path = guest_dir
        .join("target/wasm32-wasip2/release")
        .join(artifact);
    assert!(
        path.is_file(),
        "missing wasm artifact at {}",
        path.display()
    );
    path
}

/// Platform-correct cdylib artifact filename for a `[lib] name = "<stem>"`.
/// cargo emits `lib<stem>.so` on Linux, `lib<stem>.dylib` on macOS and
/// `<stem>.dll` on Windows.
pub fn dylib_name(stem: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{stem}.dll")
    }
    #[cfg(target_os = "macos")]
    {
        format!("lib{stem}.dylib")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        format!("lib{stem}.so")
    }
}

/// Build a native example plugin (workspace member) and return the path to its
/// dynamic-library artifact. `lib_stem` is the crate's `[lib] name`; the file
/// extension is resolved per platform.
pub fn build_native_example(package: &str, lib_stem: &str) -> PathBuf {
    let root = workspace_root();
    let mut cmd = cargo();
    cmd.args(["build", "-p", package]).current_dir(&root);
    run_build(cmd, package);
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let path = target.join("debug").join(dylib_name(lib_stem));
    assert!(
        path.is_file(),
        "missing native artifact at {}",
        path.display()
    );
    path
}
