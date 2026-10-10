pub(crate) fn html() -> &'static str {
    include_str!("../assets/live-app.html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_app_is_self_contained_and_under_host_limit() {
        let html = html();
        assert!(html.len() <= 2 * 1024 * 1024);
        for external in ["src=\"http", "href=\"http", "url(http", "@import"] {
            assert!(!html.to_ascii_lowercase().contains(external));
        }
    }

    #[test]
    fn packaged_app_uses_monospace_tabular_telemetry() {
        let styles = html()
            .split_once("<style>")
            .expect("packaged App has inline styles")
            .1
            .split_once("</style>")
            .expect("packaged styles terminate")
            .0;
        assert!(styles.contains("ui-monospace"));
        assert!(styles.contains("font-variant-numeric:tabular-nums"));
        assert!(styles.contains(".stats{"));
        assert!(styles.contains("font:9px/1.3 var(--mono)"));
    }

    #[test]
    fn packaged_app_contains_agent_instruction_controls() {
        let packaged = html();
        for control in [
            "<form id=\"commands\">",
            "<select id=\"agent\" aria-label=\"Agent\">",
            "<input id=\"command\" maxlength=\"16384\"",
            "<button type=\"submit\">Send instruction</button>",
            "<output id=\"command-result\">",
        ] {
            assert!(
                packaged.contains(control),
                "missing packaged control {control}"
            );
        }
    }

    #[test]
    fn app_is_self_contained_and_uses_only_simulator_live_view_tools() {
        let source = include_str!("../app/main.js");
        for expected in [
            "VideoDecoder",
            "EncodedVideoChunk",
            "veoveo.h264.annexb.v1",
            "H264DecoderAdmission",
            "list_live_cameras",
            "open_live_view",
            "renew_live_view",
            "close_live_view",
            "subscriptions/listen",
            "resourceSubscriptions",
            "ui/resource-teardown",
            "applyHostContext(initialized?.hostContext)",
            "navigator.mediaCapabilities.decodingInfo",
            "hardwareAcceleration:\"prefer-hardware\"",
            "hardwareAcceleration:\"no-preference\"",
            "section.dataset.viewerInstanceId",
            "section.dataset.streamProductId",
            "tiled H.264 live",
            "stream.codedWidthPx",
            "stream.sourceRegion",
            "camera.rig?.smoothing",
            "video pending",
            "fixedFps",
            ".padStart(2,\"0\")",
            "fixedFps(player.presentedAt.length)",
            "MAX_RECOVERY_BACKOFF_ATTEMPT=8",
            "Camera recovery is waiting for simulator readiness",
            "ensureSubscription",
            "retrySelectedNow",
            "veoveo/agents/message",
            "ai.veoveo/agent-message-targets",
            "uuidV7",
            "if(!selected.size)",
            "await open(camera)",
            "views.get(camera.cameraId)!==view",
            "section.querySelector(\".empty\")?.remove()",
        ] {
            assert!(source.contains(expected), "missing {expected}");
        }
        for removed in [
            "reconciliation",
            "pose authorization",
            "set_camera",
            "OVWebRTC",
            "AppStreamer",
            "PressureObserver",
            "avc1.42E01E",
            "avc1.4d4034",
        ] {
            assert!(!source.contains(removed), "obsolete App source {removed}");
            assert!(
                !html().contains(removed),
                "obsolete packaged App surface {removed}"
            );
        }
        assert!(source.contains("{sessionId:sessionId}"));
        assert!(!source.contains("session_id"));
        assert!(source.contains("if(name===\"open_live_view\")await ensureSubscription()"));
        assert!(!source.contains(
            "error(\"Camera recovery is waiting for simulator readiness\");status();return;"
        ));
        assert!(source.contains("sessionId=first.sessionId"));
        assert!(!source.contains("setInterval"));
    }
}
