/** These URI shapes name the same governed Artifact plane object. They grant no access. */
export function artifactId(uri: string): string | undefined {
  const match = /^(?:artifact:\/\/|([a-z][a-z0-9+.-]*):\/\/artifact\/)([0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})$/.exec(uri);
  return match && !["http", "https", "file", "data", "javascript"].includes(match[1] ?? "") ? match[2] : undefined;
}

export function artifactPath(id: string, presentation: "preview" | "download"): string {
  return `/workspace/api/artifacts/${encodeURIComponent(id)}/${presentation}`;
}

export function canPreviewImage(type: string | null, size: string | null): boolean {
  const mime = type?.split(";")[0].trim().toLowerCase();
  const length = size !== null && /^\d+$/.test(size) ? Number(size) : NaN;
  return ["image/png", "image/jpeg", "image/webp", "image/gif", "image/avif"].includes(mime ?? "") && Number.isSafeInteger(length) && length > 0 && length <= 20 * 1024 * 1024;
}
