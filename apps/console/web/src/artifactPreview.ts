export function artifactPreviewLabel(mediaType: string): string {
  const normalized = mediaType.toLowerCase();
  if (normalized === "application/vnd.rerun.rrd") return "Open in Rerun";
  if (normalized.startsWith("video/")) return "Play video";
  if (normalized.startsWith("audio/")) return "Play audio";
  if (normalized.startsWith("image/")) return "View image";
  if (normalized === "application/pdf") return "View PDF";
  if (
    normalized.startsWith("text/") ||
    normalized.includes("json") ||
    normalized.includes("xml") ||
    normalized.includes("yaml")
  ) {
    return "Read preview";
  }
  return "View details";
}
