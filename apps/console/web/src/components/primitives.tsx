import type { ReactNode } from "react";
import { ChevronRight } from "lucide-react";

export function StatusPill({ value }: { value: string }) {
  const tone = ["healthy", "succeeded", "active", "sealed", "released", "idle", "running", "bound", "completed"].includes(value)
    ? "good"
    : ["failed", "denied", "offline", "cancelled", "disabled"].includes(value)
      ? "bad"
      : ["degraded", "waiting", "queued", "draft", "cancel_requested"].includes(value)
        ? "warn"
        : "neutral";
  return (
    <span className={`status status-${tone}`}>
      <span className="status-dot" aria-hidden="true" />
      {value.replaceAll("_", " ")}
    </span>
  );
}

export function Metric({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div className="metric">
      <div className="metric-label">{label}</div>
      <div className="metric-value">{value}</div>
      <div className="metric-detail">{detail}</div>
    </div>
  );
}

export function ProgressBar({ value }: { value: number }) {
  const percent = Math.max(0, Math.min(100, Math.round(value * 100)));
  return (
    <div className="progress-wrap" aria-label={`${percent}% complete`}>
      <div className="progress-track">
        <span style={{ width: `${percent}%` }} />
      </div>
      <span className="progress-value">{percent}%</span>
    </div>
  );
}

export function SectionHeader({
  title,
  count,
  actions
}: {
  title: string;
  count?: number;
  actions?: ReactNode;
}) {
  return (
    <div className="section-header">
      <div className="section-title-row">
        <h2>{title}</h2>
        {count !== undefined && <span className="count">{count}</span>}
      </div>
      {actions && <div className="section-actions">{actions}</div>}
    </div>
  );
}

export function EmptyState({ children }: { children: ReactNode }) {
  return <div className="empty-state">{children}</div>;
}

export function RowLink() {
  return <ChevronRight size={15} aria-hidden="true" />;
}
