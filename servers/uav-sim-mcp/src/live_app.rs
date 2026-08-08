use std::sync::OnceLock;

const TEMPLATE: &str = include_str!("../assets/live-app.html");
const CLIENT: &str = include_str!(concat!(env!("OUT_DIR"), "/ov-web-rtc.umd.cjs"));
const MARKER: &str = "/*__NVIDIA_OV_WEB_RTC__*/";

pub(crate) fn html() -> &'static str {
    static HTML: OnceLock<String> = OnceLock::new();
    HTML.get_or_init(|| {
        assert_eq!(TEMPLATE.matches(MARKER).count(), 1);
        TEMPLATE.replacen(MARKER, CLIENT, 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_is_self_contained_and_uses_only_simulator_live_view_tools() {
        let html = html();
        assert!(!html.contains(MARKER));
        for expected in [
            "OVWebRTC",
            "list_live_cameras",
            "open_live_view",
            "renew_live_view",
            "close_live_view",
            "resources/subscribe",
            "resources/unsubscribe",
            "ui/resource-teardown",
            "navigator.mediaCapabilities.decodingInfo",
            "section.dataset.viewerInstanceId",
            "section.dataset.streamProductId",
            "section.dataset.capacitySlot",
            "requestVideoFrameCallback",
            "onStreamStats",
            "event?.data?.stats",
            "camera.rig?.smoothing",
            "decoded pending",
            "Google Photorealistic 3D Tiles",
            "MAX_RECOVERY_ATTEMPTS=8",
            "Camera recovery is waiting for simulator readiness",
            "retrySelectedNow",
        ] {
            assert!(html.contains(expected), "missing {expected}");
        }
        for removed in ["reconciliation", "pose authorization", "set_camera"] {
            assert!(!html.contains(removed), "obsolete App surface {removed}");
        }
        assert!(html.contains("{session_id:sessionId}"));
        assert!(!html.contains("first.sessionId"));
        assert!(!html.contains("setInterval"));
    }
}
