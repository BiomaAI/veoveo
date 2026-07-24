use std::sync::OnceLock;

const TEMPLATE: &str = include_str!("../assets/live-app.html");
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
    fn app_is_self_contained_and_drives_the_typed_lifecycle() {
        let html = live_app_html();
        assert!(!html.contains(MARKER));
        for needle in [
            "OVWebRTC",
            "ui/initialize",
            "resources/read",
            "open_live_stream",
            "renew_live_stream",
            "close_live_stream",
            "ui/resource-teardown",
            "!result.supported || !result.smooth",
            "software H.264 decode",
            "hardware H.264 decode",
        ] {
            assert!(html.contains(needle), "missing {needle}");
        }
    }
}
