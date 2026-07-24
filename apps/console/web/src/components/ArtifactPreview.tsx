import { lazy, Suspense, useEffect, useState, type FormEvent } from "react";
import {
  Box,
  Download,
  FileQuestion,
  FileText,
  Image as ImageIcon,
  LockKeyhole,
  Music2,
  Send,
  Video,
} from "lucide-react";
import { artifactDownloadUrl, artifactPreviewUrl } from "../api";
import { redirectToLogin } from "../auth";
import { useArtifactAccessRequests, useRequestArtifactAccess } from "../queries";
import type { ArtifactSummary } from "../types";

const TEXT_PREVIEW_BYTES = 256 * 1024;
const GovernedRerunArtifactViewer = lazy(
  () => import("./GovernedRerunArtifactViewer")
);

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
  const [justification, setJustification] = useState("");
  const [requestError, setRequestError] = useState<string>();
  const accessRequests = useArtifactAccessRequests("mine", "pending");
  const requestAccess = useRequestArtifactAccess();
  const pendingRequest = accessRequests.data?.requests.find(
    (request) => request.artifactId === artifact.id
  );
  const submitRequest = async (event: FormEvent) => {
    event.preventDefault();
    if (!justification.trim()) return;
    setRequestError(undefined);
    try {
      await requestAccess.mutateAsync({
        artifactId: artifact.id,
        requestedLevel: "read",
        justification: justification.trim(),
      });
      setJustification("");
    } catch (cause) {
      setRequestError(cause instanceof Error ? cause.message : "Access request failed");
    }
  };
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
          redirectToLogin();
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
          {pendingRequest ? (
            <span>
              Read access was requested {new Date(pendingRequest.createdAt).toLocaleString()}. A
              custodian for <code>{pendingRequest.workContext}</code> can review it.
            </span>
          ) : artifact.effectiveAccess.requestable ? (
            <form className="preview-access-form" onSubmit={(event) => void submitRequest(event)}>
              <label>
                <span>Business justification</span>
                <textarea
                  value={justification}
                  onChange={(event) => setJustification(event.target.value)}
                  placeholder="Describe the work that requires this artifact."
                  maxLength={4096}
                  required
                />
              </label>
              <button
                className="button button-primary"
                type="submit"
                disabled={requestAccess.isPending || !justification.trim()}
              >
                <Send size={14} /> Request read access
              </button>
              {requestError && <span className="action-error">{requestError}</span>}
            </form>
          ) : (
            <span>
              {artifact.effectiveAccess.denialReason === "clearance"
                ? "The active principal does not hold every data label required by this artifact. A discretionary grant cannot change clearance."
                : "This artifact is outside the active tenant boundary."}
            </span>
          )}
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
            <GovernedRerunArtifactViewer
              key={artifact.id}
              artifactId={artifact.id}
              url={url}
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
