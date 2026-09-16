import type { ChatAttachment } from "./generated/workspace.ts";
import { artifactId } from "./resources.ts";

export function attachmentReference(uri: string, name: string): ChatAttachment {
  const id = artifactId(uri.trim());
  if (!id) throw new Error("Use an Artifact resource link, such as artifact:// followed by its file ID.");
  const label = name.trim();
  if (!label || new TextEncoder().encode(label).length > 255 || /[\u0000-\u001f\u007f-\u009f]/.test(label)) {
    throw new Error("Choose a file name of at most 255 bytes without control characters.");
  }
  return { kind: "artifact", id, name: label };
}
