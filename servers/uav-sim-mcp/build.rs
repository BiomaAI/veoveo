use std::{env, fs, path::PathBuf};

use sha2::{Digest, Sha256};

const NVIDIA_OV_WEB_RTC_UMD_SHA256: &str =
    "ef2bab07d13bee861c30922100f9c98fd61982826fdc8cedc6e43f032d8fa70d";

fn main() {
    println!("cargo:rerun-if-env-changed=UAV_SIM_WEBRTC_CLIENT_BUNDLE");
    println!("cargo:rerun-if-changed=assets/vendor/ov-web-rtc.stub.js");
    println!("cargo:rerun-if-changed=assets/live-app.html");

    let production_source = env::var_os("UAV_SIM_WEBRTC_CLIENT_BUNDLE");
    let source = production_source
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/vendor/ov-web-rtc.stub.js"));
    let bytes = fs::read(&source).unwrap_or_else(|error| {
        panic!(
            "failed to read NVIDIA OV WebRTC client from {}: {error}",
            source.display()
        )
    });
    if production_source.is_some() {
        let actual = Sha256::digest(&bytes);
        let mut actual_hex = String::with_capacity(64);
        for byte in actual {
            use std::fmt::Write as _;
            write!(actual_hex, "{byte:02x}").expect("writing to a String cannot fail");
        }
        assert_eq!(
            actual_hex, NVIDIA_OV_WEB_RTC_UMD_SHA256,
            "UAV_SIM_WEBRTC_CLIENT_BUNDLE does not match the pinned NVIDIA \
             @nvidia/ov-web-rtc 6.6.0 UMD dependency"
        );
    }
    let target =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set")).join("ov-web-rtc.umd.cjs");
    fs::write(&target, bytes).unwrap_or_else(|error| {
        panic!(
            "failed to embed NVIDIA OV WebRTC client from {}: {error}",
            source.display()
        )
    });
}
