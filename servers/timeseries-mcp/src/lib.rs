pub mod artifacts;
pub mod contract;
pub mod forecast;
pub mod state;
pub mod uris;

#[cfg(test)]
mod forecast_app_tests {
    const FORECAST_APP_HTML: &str = include_str!("../assets/forecast-app.html");
    const MAX_APP_HTML_BYTES: usize = 2 * 1024 * 1024;

    /// The app must work under the host's deny-all frame CSP: no
    /// fetch-capable reference to any external origin.
    #[test]
    fn forecast_app_is_self_contained() {
        let lowered = FORECAST_APP_HTML.to_ascii_lowercase();
        for needle in [
            "src=\"http://",
            "src=\"https://",
            "src='http://",
            "src='https://",
            "href=\"http://",
            "href=\"https://",
            "href='http://",
            "href='https://",
            "url(http://",
            "url(https://",
            "@import",
        ] {
            assert!(
                !lowered.contains(needle),
                "forecast app references an external origin via `{needle}`"
            );
        }
    }

    #[test]
    fn forecast_app_stays_under_the_host_size_cap() {
        assert!(FORECAST_APP_HTML.len() < MAX_APP_HTML_BYTES);
        assert!(FORECAST_APP_HTML.contains("ui/initialize"));
        assert!(FORECAST_APP_HTML.contains("tools/call"));
    }
}
