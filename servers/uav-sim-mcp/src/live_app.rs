pub(crate) fn html() -> &'static str {
    include_str!("../assets/live-app.html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_is_self_contained_and_uses_only_simulator_live_view_tools() {
        let html = html();
        for expected in [
            "VideoDecoder",
            "EncodedVideoChunk",
            "veoveo.h264.annexb.v1",
            "avc1.640033",
            "list_live_cameras",
            "open_live_view",
            "renew_live_view",
            "close_live_view",
            "subscriptions/listen",
            "resourceSubscriptions",
            "ui/resource-teardown",
            "applyHostContext(initialized?.hostContext)",
            "navigator.mediaCapabilities.decodingInfo",
            "section.dataset.viewerInstanceId",
            "section.dataset.streamProductId",
            "tiled H.264 live",
            "stream.codedWidthPx",
            "stream.sourceRegion",
            "camera.rig?.smoothing",
            "video pending",
            "font-variant-numeric:tabular-nums",
            "ui-monospace",
            "fixedFps",
            ".padStart(2,\"0\")",
            "fixedFps(player.presentedAt.length)",
            "Google Photorealistic 3D Tiles",
            "MAX_RECOVERY_BACKOFF_ATTEMPT=8",
            "Camera recovery is waiting for simulator readiness",
            "ensureSubscription",
            "retrySelectedNow",
            "veoveo/agents/message",
            "io.veoveo/agent-message-targets",
            "uuidV7",
            "Send instruction",
            "if(!selected.size)",
            "await open(camera)",
        ] {
            assert!(html.contains(expected), "missing {expected}");
        }
        for removed in [
            "reconciliation",
            "pose authorization",
            "set_camera",
            "OVWebRTC",
            "AppStreamer",
            "PressureObserver",
            "avc1.42E01E",
        ] {
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
}
