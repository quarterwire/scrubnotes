//! Copies libmpv from `libmpv/` next to the built executable so `cargo run` just works.

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=libmpv");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest.join("libmpv").join("libmpv-2.dll");
    let Ok(src_meta) = fs::metadata(&src) else {
        println!("cargo:warning=libmpv/libmpv-2.dll not found; run scripts/fetch-libmpv.ps1 before running the app");
        return;
    };
    // OUT_DIR is target/<profile>/build/<pkg>-<hash>/out; the executable lives in target/<profile>.
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let Some(profile_dir) = out.ancestors().nth(3) else { return };
    let dst = profile_dir.join("libmpv-2.dll");
    let up_to_date = fs::metadata(&dst).is_ok_and(|d| d.len() == src_meta.len());
    if !up_to_date && let Err(e) = fs::copy(&src, &dst) {
        println!("cargo:warning=could not copy libmpv-2.dll to {}: {e}", dst.display());
    }
}
