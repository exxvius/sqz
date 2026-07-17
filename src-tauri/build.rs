fn main() {
    // Expose the build target triple to the crate so ffbin.rs can locate the
    // dev sidecar binaries (named `ffmpeg-<triple>` etc.).
    if let Ok(target) = std::env::var("TARGET") {
        println!("cargo:rustc-env=TARGET={target}");
    }
    tauri_build::build();
}
