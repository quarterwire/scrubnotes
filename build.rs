//! Windows build steps:
//! - copies libmpv from `libmpv/` next to the built executable so `cargo run` just works;
//! - embeds the app icon and version info (shown by Explorer, the Start menu, the taskbar and
//!   Task Manager) using MinGW's windres, skipped with a warning if it isn't available.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

const APP_NAME: &str = "ScrubNotes";

fn main() {
    println!("cargo:rerun-if-changed=libmpv");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    copy_libmpv(&manifest, &out);
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        println!("cargo:warning=icon/version resources are only embedded for the windows-gnu target");
    } else if let Err(e) = embed_resources(&manifest, &out) {
        println!("cargo:warning=skipping icon/version resources: {e}");
    }
}

fn copy_libmpv(manifest: &Path, out: &Path) {
    let src = manifest.join("libmpv").join("libmpv-2.dll");
    let Ok(src_meta) = fs::metadata(&src) else {
        println!("cargo:warning=libmpv/libmpv-2.dll not found; run scripts/fetch-libmpv.ps1 before running the app");
        return;
    };
    // OUT_DIR is target/<profile>/build/<pkg>-<hash>/out; the executable lives in target/<profile>.
    let Some(profile_dir) = out.ancestors().nth(3) else { return };
    let dst = profile_dir.join("libmpv-2.dll");
    let up_to_date = fs::metadata(&dst).is_ok_and(|d| d.len() == src_meta.len());
    if !up_to_date && let Err(e) = fs::copy(&src, &dst) {
        println!("cargo:warning=could not copy libmpv-2.dll to {}: {e}", dst.display());
    }
}

fn embed_resources(manifest: &Path, out: &Path) -> Result<(), String> {
    let windres = find_windres().ok_or("windres not found (install MinGW or set WINDRES)")?;
    let icon = manifest.join("assets").join("icon.ico");
    let v = |k: &str| env::var(k).unwrap_or_default();
    let (version, major, minor, patch) =
        (v("CARGO_PKG_VERSION"), v("CARGO_PKG_VERSION_MAJOR"), v("CARGO_PKG_VERSION_MINOR"), v("CARGO_PKG_VERSION_PATCH"));
    let rc = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "quarterwire"
      VALUE "FileDescription", "{APP_NAME}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{pkg}"
      VALUE "OriginalFilename", "{APP_NAME}.exe"
      VALUE "ProductName", "{APP_NAME}"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = icon.display().to_string().replace('\\', "/"),
        pkg = v("CARGO_PKG_NAME"),
    );
    let rc_path = out.join("app.rc");
    let obj = out.join("app-resources.o");
    fs::write(&rc_path, rc).map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&windres);
    // windres runs the MinGW preprocessor, which lives next to it.
    if let Some(dir) = windres.parent().filter(|d| !d.as_os_str().is_empty()) {
        let path = env::var_os("PATH").unwrap_or_default();
        let mut dirs = vec![dir.to_path_buf()];
        dirs.extend(env::split_paths(&path));
        cmd.env("PATH", env::join_paths(dirs).map_err(|e| e.to_string())?);
    }
    let status = cmd
        .arg("--input")
        .arg(&rc_path)
        .arg("--output-format=coff")
        .arg("--output")
        .arg(&obj)
        .status()
        .map_err(|e| format!("couldn't run {}: {e}", windres.display()))?;
    if !status.success() {
        return Err(format!("windres failed ({status})"));
    }
    println!("cargo:rustc-link-arg-bins={}", obj.display());
    Ok(())
}

fn find_windres() -> Option<PathBuf> {
    if let Some(p) = env::var_os("WINDRES") {
        return Some(PathBuf::from(p));
    }
    let on_path = Command::new("windres").arg("--version").output().is_ok_and(|o| o.status.success());
    if on_path {
        return Some(PathBuf::from("windres"));
    }
    ["C:/ProgramData/mingw64/mingw64/bin/windres.exe", "C:/msys64/mingw64/bin/windres.exe", "C:/msys64/ucrt64/bin/windres.exe"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}
