use std::sync::OnceLock;

const TEMPLATE: &str = include_str!("../assets/live.html");
const CLIENT: &str = include_str!(concat!(env!("OUT_DIR"), "/ov-web-rtc.umd.cjs"));
const MARKER: &str = "/*__NVIDIA_OV_WEB_RTC__*/";

pub(crate) fn live_app_html() -> &'static str {
    static HTML: OnceLock<String> = OnceLock::new();
    HTML.get_or_init(|| {
        assert_eq!(
            TEMPLATE.matches(MARKER).count(),
            1,
            "live App must contain exactly one NVIDIA client marker"
        );
        TEMPLATE.replacen(MARKER, CLIENT, 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_is_self_contained_and_uses_canonical_live_view_tools() {
        let html = live_app_html();
        assert!(!html.contains(MARKER));
        for needle in [
            "OVWebRTC",
            "ui/initialize",
            "resources/read",
            "open_live_view",
            "renew_live_view",
            "close_live_view",
            "set_camera",
            "ui/resource-teardown",
            "ResizeObserver",
            "navigator.mediaCapabilities.decodingInfo",
            "software H.264 decode",
            "hardware H.264 decode",
            "maximumVisibleViews",
        ] {
            assert!(html.contains(needle), "missing {needle}");
        }
        for domain_control in [
            "pause_simulation",
            "resume_simulation",
            "reset_simulation",
            "step_simulation",
            "apply_control",
        ] {
            assert!(
                !html.contains(domain_control),
                "generic App contains domain control {domain_control}"
            );
        }
    }
}
