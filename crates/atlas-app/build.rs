//! Records how the binary was built so the perf log can say whether a slow
//! frame came from a debug build (opt-level 0 for our crates) or a release one.
//! Cargo only exposes `PROFILE` and `OPT_LEVEL` to build scripts, hence this.

fn main() {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
    let opt_level = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "?".into());
    println!("cargo:rustc-env=ATLAS_BUILD_PROFILE={profile}");
    println!("cargo:rustc-env=ATLAS_BUILD_OPT_LEVEL={opt_level}");
    println!("cargo:rerun-if-changed=build.rs");
}
