import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import {
  Box,
  Download,
  FileQuestion,
  FileText,
  Image as ImageIcon,
  LockKeyhole,
  Music2,
  Video,
} from "lucide-react";
import { artifactDownloadUrl, artifactPreviewUrl } from "../api";
import type { ArtifactSummary } from "../types";

const TEXT_PREVIEW_BYTES = 256 * 1024;
const GovernedRerunViewer = lazy(() => import("./GovernedRerunViewer"));

export function ArtifactPreview({
  artifact,
  principalId,
}: {
  artifact: ArtifactSummary;
  principalId: string;
}) {
  const mediaType = artifact.mediaType.toLowerCase();
  if (!hasInlinePreview(mediaType)) {
    return (
      <div className="artifact-preview artifact-preview-unsupported">
        <FileQuestion size={24} />
        <div>
          <strong>No inline renderer for {artifact.mediaType}</strong>
          <span>The governed file remains available for download.</span>
        </div>
        <a className="button button-secondary" href={artifactDownloadUrl(artifact.id)}>
          <Download size={14} /> Download
        </a>
      </div>
    );
  }
  return (
    <AuthorizedArtifactPreview
      artifact={artifact}
      mediaType={mediaType}
      principalId={principalId}
    />
  );
}

function AuthorizedArtifactPreview({
  artifact,
  mediaType,
  principalId,
}: {
  artifact: ArtifactSummary;
  mediaType: string;
  principalId: string;
}) {
  const url = artifactPreviewUrl(artifact.id);
  const [access, setAccess] = useState<"checking" | "allowed" | "denied" | "unavailable">(
    "checking"
  );
  const [accessDetail, setAccessDetail] = useState<string>();
  const [mediaError, setMediaError] = useState<string>();
  const rerunSegments = useMemo(
    () => [{ ordinal: 0, byteLength: artifact.byteLength, url }],
    [artifact.byteLength, url]
  );

  useEffect(() => {
    const controller = new AbortController();
    void fetch(url, {
      credentials: "same-origin",
      headers: { Range: "bytes=0-0" },
      signal: controller.signal,
    })
      .then(async (response) => {
        await response.body?.cancel();
        if (response.status === 401) {
          window.location.assign("/auth/login");
          return;
        }
        if (response.status === 403) {
          setAccess("denied");
          return;
        }
        if (!response.ok) {
          setAccessDetail(`Preview authorization returned ${response.status}.`);
          setAccess("unavailable");
          return;
        }
        setAccess("allowed");
      })
      .catch((cause: unknown) => {
        if (!controller.signal.aborted) {
          setAccessDetail(
            cause instanceof Error ? cause.message : "Preview authorization failed."
          );
          setAccess("unavailable");
        }
      });
    return () => controller.abort();
  }, [url]);

  if (access === "checking") {
    return (
      <div className="artifact-preview artifact-preview-loading artifact-preview-checking">
        <div className="loading-mark" /> Checking governed preview access…
      </div>
    );
  }
  if (access === "denied") {
    return (
      <div className="artifact-preview artifact-preview-denied">
        <LockKeyhole size={25} />
        <div>
          <strong>Preview access required</strong>
          <span>
            This private artifact is owned by <code>{artifact.owner}</code>. The active Console
            principal <code>{principalId}</code> does not have a read grant.
          </span>
          <span>Ask an artifact administrator to grant read access, then reopen this preview.</span>
        </div>
      </div>
    );
  }
  if (access === "unavailable") {
    return (
      <div className="artifact-preview artifact-preview-denied">
        <FileQuestion size={25} />
        <div>
          <strong>Preview service unavailable</strong>
          <span>{accessDetail ?? "The governed preview could not be authorized."}</span>
        </div>
      </div>
    );
  }

  if (mediaType === "application/vnd.rerun.rrd") {
    return (
      <div className="artifact-preview artifact-preview-rerun">
        <div className="artifact-preview-label"><Box size={13} /> Interactive Rerun preview</div>
        <div className="artifact-rerun-viewer">
          <Suspense fallback={<div className="artifact-preview-loading"><div className="loading-mark" /> Loading Rerun 0.34.1…</div>}>
            <GovernedRerunViewer
              key={artifact.id}
              recordingId={`artifact ${artifact.id}`}
              segments={rerunSegments}
            />
          </Suspense>
        </div>
      </div>
    );
  }

  if (mediaType.startsWith("image/")) {
    return (
      <div className="artifact-preview artifact-preview-media">
        {mediaError
          ? <span className="artifact-preview-error">{mediaError}</span>
          : <img src={url} alt={`Preview of ${artifact.filename}`} onError={() => setMediaError("The image preview could not be loaded with this session's access.")} />}
        <span><ImageIcon size={13} /> Image preview</span>
      </div>
    );
  }
  if (mediaType.startsWith("video/")) {
    return (
      <div className="artifact-preview artifact-preview-media">
        {mediaError
          ? <span className="artifact-preview-error">{mediaError}</span>
          : <video src={url} controls preload="metadata" onError={() => setMediaError("The video preview could not be loaded with this session's access.")} />}
        <span><Video size={13} /> Video preview</span>
      </div>
    );
  }
  if (mediaType.startsWith("audio/")) {
    return (
      <div className="artifact-preview artifact-preview-audio">
        <Music2 size={24} />
        {mediaError
          ? <span className="artifact-preview-error">{mediaError}</span>
          : <audio src={url} controls preload="metadata" onError={() => setMediaError("The audio preview could not be loaded with this session's access.")} />}
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

function hasInlinePreview(mediaType: string): boolean {
  return (
    mediaType === "application/vnd.rerun.rrd" ||
    mediaType.startsWith("image/") ||
    mediaType.startsWith("video/") ||
    mediaType.startsWith("audio/") ||
    mediaType === "application/pdf" ||
    mediaType.startsWith("text/") ||
    mediaType.includes("json") ||
    mediaType.includes("xml") ||
    mediaType.includes("yaml")
  );
}
