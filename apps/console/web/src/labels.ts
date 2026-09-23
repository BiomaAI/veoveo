/** Display labels for enum values the Console receives from the platform. */

const recoveryClassLabels: Record<string, string> = {
  resume: "Resumes after restart",
  webhook_wait: "Waits for provider callback",
  provider_wait: "Waits for provider result",
  interrupted_indeterminate: "Not retried if interrupted",
};

export function recoveryClassLabel(value: string): string {
  return recoveryClassLabels[value] ?? humanize(value);
}

const statusLabels: Record<string, string> = {
  budget_terminated: "stopped at budget limit",
  cancel_requested: "cancelling",
};

export function statusLabel(value: string): string {
  return statusLabels[value] ?? humanize(value);
}

const invocationModeLabels: Record<string, string> = {
  direct: "Direct, by a person",
  delegated: "On behalf of a person",
  automated: "Automated",
};

export function invocationModeLabel(value: string): string {
  return invocationModeLabels[value] ?? humanize(value);
}

const instanceTargetLabels: Record<string, string> = {
  running: "Running",
  paused: "Paused",
  archived: "Archived",
};

const instancePhaseLabels: Record<string, string> = {
  queued: "Queued",
  credentials: "Preparing credentials",
  storage: "Preparing storage",
  draining: "Finishing current run",
  workload: "Starting",
  ready: "Ready",
  paused: "Paused",
  archived: "Archived",
  failed: "Failed",
  superseded: "Replaced by a newer request",
};

/** "Target: Running · Finishing current run" for a managed agent instance. */
export function instanceStatusLabel(target: string, phase: string): string {
  return `Target: ${instanceTargetLabels[target] ?? humanize(target)} · ${instancePhaseLabels[phase] ?? humanize(phase)}`;
}

function humanize(value: string): string {
  return value.replaceAll("_", " ");
}
