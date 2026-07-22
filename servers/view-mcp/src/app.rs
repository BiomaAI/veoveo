use std::sync::LazyLock;

/// Splice marker for the vendored three.js bundle; must appear exactly once,
/// alone inside a `<script>` element of the template.
const THREE_BUNDLE_MARKER: &str = "/*__VEOVEO_THREE_BUNDLE__*/";

/// The console host rejects app documents above 2 MiB
/// (`apps/console/bff/src/apps.rs` `MAX_APP_HTML_BYTES`); the margin below it
/// absorbs template growth without silently outgrowing the host.
#[cfg(test)]
const MAX_COMPOSED_BYTES: usize = 1_900_000;

static PREVIEW_APP_HTML: LazyLock<String> = LazyLock::new(|| {
    let template = include_str!("../assets/preview-app.template.html");
    debug_assert_eq!(template.matches(THREE_BUNDLE_MARKER).count(), 1);
    template.replacen(
        THREE_BUNDLE_MARKER,
        include_str!("../assets/vendor/three-bundle.min.js"),
        1,
    )
});

pub(crate) fn preview_app_html() -> &'static str {
    &PREVIEW_APP_HTML
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_app_is_self_contained() {
        let html = preview_app_html();
        for needle in [
            "src=\"http",
            "src='http",
            "href=\"http",
            "href='http",
            "url(http",
            "@import",
        ] {
            assert!(
                !html.contains(needle),
                "app must not reference external origins: {needle}"
            );
        }
    }

    #[test]
    fn preview_app_speaks_the_bridge_protocol() {
        let html = preview_app_html();
        for needle in ["ui/initialize", "tools/call", "resources/read", "tasks/get"] {
            assert!(html.contains(needle), "app must contain {needle}");
        }
    }

    #[test]
    fn preview_app_uses_host_tool_identity_and_initial_result() {
        let html = preview_app_html();
        for needle in [
            "hostContext.toolInfo.tool.name",
            "projectedToolName(name)",
            "ui/notifications/tool-input",
            "ui/notifications/tool-result",
            "result.structuredContent",
        ] {
            assert!(html.contains(needle), "app must contain {needle}");
        }
    }

    #[test]
    fn preview_app_stays_under_the_host_size_cap() {
        assert!(
            preview_app_html().len() < MAX_COMPOSED_BYTES,
            "composed app is {} bytes",
            preview_app_html().len()
        );
    }

    #[test]
    fn vendor_bundle_was_spliced_and_is_script_safe() {
        let html = preview_app_html();
        assert!(!html.contains(THREE_BUNDLE_MARKER));
        assert!(html.contains("DracoDecoderModule"));
        let bundle = include_str!("../assets/vendor/three-bundle.min.js");
        assert!(
            !bundle.to_ascii_lowercase().contains("</script"),
            "bundle would terminate its enclosing <script> element"
        );
    }

    #[test]
    fn preview_app_uses_the_contract_camera_shapes() {
        let html = preview_app_html();
        for needle in [
            "\"pose\"",
            "\"look_at\"",
            "\"orbit_target\"",
            "vertical_fov_degrees",
            "view://layers",
        ] {
            assert!(html.contains(needle), "app must contain {needle}");
        }
    }

    #[test]
    fn preview_app_keeps_orientation_without_a_reference_grid() {
        let html = preview_app_html();
        assert!(!html.contains("THREE.GridHelper"));
        assert!(
            html.contains("this.compass.position.set(localPoint[0], localPoint[1], localPoint[2])")
        );
        assert!(html.contains("this.helper = new THREE.CameraHelper"));
    }
}
