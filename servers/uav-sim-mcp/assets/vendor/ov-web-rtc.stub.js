(function (root) {
  "use strict";
  root.__VEOVEO_WEBRTC_STUB__ = true;
  class AppStreamer {
    connect() {
      throw new Error(
        "The NVIDIA OV WebRTC production bundle was not embedded. Build the production image."
      );
    }
    terminate() {}
  }
  root.OVWebRTC = {
    AppStreamer,
    StreamType: { DIRECT: "direct" },
    LogLevel: { WARN: "warn" },
    EventStatus: { SUCCESS: "success" },
    VideoCodec: { H264: "h264" },
  };
})(globalThis);
