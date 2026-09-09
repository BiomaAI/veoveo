/** Each job reads at most one admitted part; file bytes never enter React state. */
self.onmessage = async (event: MessageEvent<{ id: number; blob: Blob }>) => {
  const { id, blob } = event.data;
  try {
    const digest = await crypto.subtle.digest("SHA-256", await blob.arrayBuffer());
    const sha = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
    self.postMessage({ id, sha });
  } catch { self.postMessage({ id, error: "This file could not be read. Select it again." }); }
};
export {};
