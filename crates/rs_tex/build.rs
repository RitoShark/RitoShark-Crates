/*!
`intel_tex_2` bundles precompiled ISPC/C++ object files for BC encoding. On Unix targets those
objects reference the C++ runtime (e.g. `__gxx_personality_v0`), so the final binary must link
the C++ standard library explicitly — GNU/Linux needs `stdc++`, Apple needs `c++`. The MSVC
toolchain links the C++ runtime automatically, so nothing is emitted there.
*/

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    match target_os.as_str() {
        "linux" | "android" => println!("cargo:rustc-link-lib=dylib=stdc++"),
        "macos" | "ios" => println!("cargo:rustc-link-lib=dylib=c++"),
        "windows" if target_env == "gnu" => println!("cargo:rustc-link-lib=dylib=stdc++"),
        _ => {}
    }
}
