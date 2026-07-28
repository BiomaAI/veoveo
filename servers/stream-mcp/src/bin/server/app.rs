pub(super) const LIVE_APP_HTML: &str = include_str!("../../../assets/live.html");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_app_uses_only_canonical_stream_surfaces() {
        for required in [
            "ui/initialize",
            "resources/read",
            "start_live_session",
            "stop_live_session",
            "stream://sessions",
            "/preview",
            "VideoDecoder",
            "EncodedVideoChunk",
            "navigator.mediaCapabilities.decodingInfo",
            "prefer-hardware",
            "prefer-software",
            "hardwareAcceleration",
            "software H.264 decode",
            "hardware H.264 decode",
            "#empty[hidden]{display:none}",
            "recording route off",
            "Live stream session",
            "ui/resource-teardown",
            "ResizeObserver",
        ] {
            assert!(LIVE_APP_HTML.contains(required), "missing {required}");
        }
        for forbidden in ["http://", "https://", "analyze_recording", "extract_clip"] {
            assert!(
                !LIVE_APP_HTML.contains(forbidden),
                "App contains forbidden surface {forbidden}"
            );
        }
    }
}
