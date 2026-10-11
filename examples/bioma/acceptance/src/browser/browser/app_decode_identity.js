(async () => {
  const canvas = document.querySelector(".view canvas");
  if (!canvas) throw new Error("shared H.264 video canvas is unavailable");
  const codec = canvas.dataset.codec;
  const width = Number(canvas.dataset.sourceWidth), height = Number(canvas.dataset.sourceHeight), framerate = Number(canvas.dataset.frameRate);
  if (!/^avc1\.[0-9a-f]{6}$/i.test(codec ?? "") || !Number.isInteger(width) || width <= 0 || !Number.isInteger(height) || height <= 0 || !Number.isFinite(framerate) || framerate <= 0)
    throw new Error("current H.264 decoder configuration is unavailable");
  // Match the actual UAV App decoderConfig MediaCapabilities assessment profile.
  const result = await navigator.mediaCapabilities.decodingInfo({ type: "file", video: {
    contentType: `video/mp4; codecs="${codec}"`, width, height, bitrate: 12000000, framerate,
  } });
  return { supported: result.supported, smooth: result.smooth, powerEfficient: result.powerEfficient,
    label: document.getElementById("decode")?.textContent ?? "" };
})()
