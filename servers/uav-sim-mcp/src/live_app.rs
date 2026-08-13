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
            "subscriptions/listen",
            "resourceSubscriptions",
            "ui/resource-teardown",
            "navigator.mediaCapabilities.decodingInfo",
            "section.dataset.viewerInstanceId",
            "section.dataset.streamProductId",
            "section.dataset.capacitySlot",
            "requestVideoFrameCallback",
            "onStreamStats",
            "event?.data?.stats",
            "camera.rig?.smoothing",
            "video pending",
            "font-variant-numeric:tabular-nums",
            "ui-monospace",
            "fixedFps",
            ".padStart(2,\"0\")",
            "fixedFps(stats.fps)",
            "Google Photorealistic 3D Tiles",
            "MAX_RECOVERY_BACKOFF_ATTEMPT=8",
            "Camera recovery is waiting for simulator readiness",
            "ensureSubscription",
            "retrySelectedNow",
        ] {
            assert!(html.contains(expected), "missing {expected}");
        }
        for removed in ["reconciliation", "pose authorization", "set_camera"] {
            assert!(!html.contains(removed), "obsolete App surface {removed}");
        }
        assert!(html.contains("{session_id:sessionId}"));
        assert!(html.contains("if(name===\"open_live_view\")await ensureSubscription()"));
        assert!(!html.contains(
            "error(\"Camera recovery is waiting for simulator readiness\");status();return;"
        ));
        assert!(!html.contains("first.sessionId"));
        assert!(!html.contains("setInterval"));
    }

    #[test]
    fn app_disables_optional_compute_pressure_before_native_client_initialization() {
        let pressure_fence = TEMPLATE
            .find("Object.defineProperty(globalThis,\"PressureObserver\"")
            .expect("App disables the optional browser telemetry");
        let client = TEMPLATE.find(MARKER).expect("native client marker exists");

        assert!(pressure_fence < client);
        assert!(TEMPLATE.contains("value:undefined"));
    }
}
