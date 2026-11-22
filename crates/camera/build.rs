// Lets Linux builds run on older systems (glibc 2.34+). See shim.c.
fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "linux" && env == "gnu" {
        cc::Build::new().file("shim.c").cargo_metadata(false).compile("xrpshim");
        println!("cargo:rustc-link-search=native={}", std::env::var("OUT_DIR").unwrap());
        println!("cargo:rustc-link-lib=static:+whole-archive=xrpshim");
    }
    println!("cargo:rerun-if-changed=shim.c");
}
