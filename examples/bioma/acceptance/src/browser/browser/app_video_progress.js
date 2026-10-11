(async () => {
  const canvas = document.querySelector(".view canvas"), view = canvas?.closest(".view");
  if (!canvas || !view) throw new Error("shared H.264 video canvas is unavailable");
  const snapshot = () => ({
    documentEpochMs: performance.timeOrigin,
    cameraId: view.id.startsWith("view-") ? view.id.slice(5) : "",
    viewerInstanceId: view.dataset.viewerInstanceId ?? "",
    liveViewId: view.dataset.liveViewId ?? "",
    streamProductId: view.dataset.streamProductId ?? "",
    decoded: Number(canvas.dataset.decodedFrames ?? 0),
    mediaTime: Number(canvas.dataset.mediaTimeSeconds ?? 0),
    now: performance.now(),
  });
  const first = snapshot(), deadline = Date.now() + 30000;
  if (!Number.isFinite(first.decoded) || first.decoded < 1 || !Number.isFinite(first.mediaTime) || first.mediaTime <= 0)
    throw new Error("restart continuity requires an admitted initial video frame");
  let last = first;
  for (let index = 0; index < 3; index++) {
    last = await new Promise((resolve, reject) => {
      const remaining = deadline - Date.now();
      if (remaining <= 0) { reject(new Error("restart video did not advance before its deadline")); return; }
      const observer = new MutationObserver(() => {
        const current = snapshot();
        for (const field of ["documentEpochMs", "cameraId", "viewerInstanceId", "liveViewId", "streamProductId"]) {
          if (current[field] !== first[field]) {
            clearTimeout(timer); observer.disconnect(); reject(new Error("restart video identity changed during continuity sample")); return;
          }
        }
        if (!Number.isFinite(current.decoded) || current.decoded < last.decoded || !Number.isFinite(current.mediaTime) || current.mediaTime < last.mediaTime) {
          clearTimeout(timer); observer.disconnect(); reject(new Error("restart video counters moved backwards")); return;
        }
        if (current.decoded <= last.decoded || current.mediaTime <= last.mediaTime) return;
        clearTimeout(timer); observer.disconnect(); resolve(current);
      });
      const timer = setTimeout(() => { observer.disconnect(); reject(new Error("restart video did not advance before its deadline")); }, remaining);
      observer.observe(canvas, { attributes: true, attributeFilter: ["data-decoded-frames"] });
    });
  }
  return { first, last, decodedFrameChanges: 3 };
})()
