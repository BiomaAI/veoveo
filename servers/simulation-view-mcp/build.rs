use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=SIMULATION_VIEW_WEBRTC_CLIENT_BUNDLE");
    println!("cargo:rerun-if-changed=assets/vendor/ov-web-rtc.stub.js");
    println!("cargo:rerun-if-changed=assets/live.html");

    let source = env::var_os("SIMULATION_VIEW_WEBRTC_CLIENT_BUNDLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/vendor/ov-web-rtc.stub.js"));
    let target =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set")).join("ov-web-rtc.umd.cjs");
    fs::copy(&source, &target).unwrap_or_else(|error| {
        panic!(
            "failed to embed NVIDIA OV WebRTC client from {}: {error}",
            source.display()
        )
    });
}
