import { browserApiRoot } from "./browserApp.ts";

export function artifactDownloadUrl(artifactId: string): string {
  return `${browserApiRoot()}/artifacts/${encodeURIComponent(artifactId)}/download`;
}

export function artifactPreviewUrl(artifactId: string): string {
  return `${browserApiRoot()}/artifacts/${encodeURIComponent(artifactId)}/preview`;
}

