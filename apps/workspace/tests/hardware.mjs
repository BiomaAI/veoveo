import assert from "node:assert/strict";

export async function hardware(page) {
  const proof = await page.evaluate(async () => {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl2", { failIfMajorPerformanceCaveat: true }) ?? canvas.getContext("webgl", { failIfMajorPerformanceCaveat: true });
    const debug = gl?.getExtension("WEBGL_debug_renderer_info");
    const renderer = debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : "";
    const adapter = await navigator.gpu?.requestAdapter({ powerPreference: "high-performance" });
    const info = adapter?.info;
    gl?.getExtension("WEBGL_lose_context")?.loseContext();
    return { userAgent: navigator.userAgent, webgl: renderer,
      webgpu: info ? { vendor: info.vendor, architecture: info.architecture, description: info.description, fallback: info.isFallbackAdapter } : null };
  });
  assert.doesNotMatch(proof.userAgent, /HeadlessChrome/);
  const software = /swiftshader|llvmpipe|software/i;
  const gl = !!proof.webgl && !software.test(proof.webgl);
  const gpu = proof.webgpu && !proof.webgpu.fallback && !!proof.webgpu.vendor && !software.test(JSON.stringify(proof.webgpu));
  assert.ok(gl || gpu, "A headed hardware WebGPU or WebGL context is required");
  return proof;
}

