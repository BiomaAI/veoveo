import { useEffect, useState } from "react";
import { FileText, Image as ImageIcon, Music2, Video } from "lucide-react";
import { artifactPreviewUrl } from "../api";
import type { ArtifactSummary } from "../types";

const TEXT_PREVIEW_BYTES = 256 * 1024;

export function ArtifactPreview({ artifact }: { artifact: ArtifactSummary }) {
  const url = artifactPreviewUrl(artifact.id);
  const mediaType = artifact.mediaType.toLowerCase();

  if (mediaType.startsWith("image/")) {
    return (
      <div className="artifact-preview artifact-preview-media">
        <img src={url} alt={`Preview of ${artifact.filename}`} />
        <span><ImageIcon size={13} /> Image preview</span>
      </div>
    );
  }
  if (mediaType.startsWith("video/")) {
    return (
      <div className="artifact-preview artifact-preview-media">
        <video src={url} controls preload="metadata" />
        <span><Video size={13} /> Video preview</span>
      </div>
    );
  }
  if (mediaType.startsWith("audio/")) {
    return (
      <div className="artifact-preview artifact-preview-audio">
        <Music2 size={24} />
        <audio src={url} controls preload="metadata" />
      </div>
    );
  }
  if (mediaType === "application/pdf") {
    return (
      <div className="artifact-preview artifact-preview-pdf">
        <iframe src={url} title={`Preview of ${artifact.filename}`} />
      </div>
    );
  }
  if (
    mediaType.startsWith("text/") ||
    mediaType.includes("json") ||
    mediaType.includes("xml") ||
    mediaType.includes("yaml")
  ) {
    return <TextPreview url={url} />;
  }
  return null;
}

function TextPreview({ url }: { url: string }) {
  const [text, setText] = useState<string>();
  const [error, setError] = useState<string>();
  const [truncated, setTruncated] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    void fetch(url, {
      credentials: "same-origin",
      headers: { Range: `bytes=0-${TEXT_PREVIEW_BYTES - 1}` },
      signal: controller.signal,
    })
      .then(async (response) => {
        if (response.status === 403) {
          throw new Error("Preview access is not granted to this console session.");
        }
        if (!response.ok) throw new Error(`Preview returned ${response.status}`);
        setTruncated(
          response.status === 206 ||
            Number(response.headers.get("content-length") ?? 0) >= TEXT_PREVIEW_BYTES
        );
        setText(await response.text());
      })
      .catch((cause: unknown) => {
        if (!controller.signal.aborted) {
          setError(cause instanceof Error ? cause.message : "Preview failed");
        }
      });
    return () => controller.abort();
  }, [url]);

  return (
    <div className="artifact-preview artifact-preview-text">
      <div><FileText size={13} /> {truncated ? "First 256 KB" : "Text preview"}</div>
      {error ? <span>{error}</span> : text === undefined ? <span>Loading preview…</span> : <pre>{text}</pre>}
    </div>
  );
}
