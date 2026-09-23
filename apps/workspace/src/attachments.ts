import type { ChatAttachment } from "./generated/workspace.ts";
import { artifactId } from "./resources.ts";

export function attachmentReference(uri: string, name: string): ChatAttachment {
  const id = artifactId(uri.trim());
  if (!id) throw new Error("Paste a file link that starts with artifact:// followed by the file ID.");
  const label = name.trim();
  if (!label) throw new Error("Enter a name to show in the chat.");
  if (new TextEncoder().encode(label).length > 255) throw new Error("Shorten the name. It can be at most 255 bytes.");
  if (/[\u0000-\u001f\u007f-\u009f]/.test(label)) throw new Error("Remove line breaks and other control characters from the name.");
  return { kind: "artifact", id, name: label };
}
