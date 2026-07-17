import type { InstallationSnapshot, McpServerSummary } from "./types";

const now = Date.now();
const ago = (minutes: number) => new Date(now - minutes * 60_000).toISOString();
const mcpServer = (
  id: string,
  toolCount: number,
  resourceCount: number,
  promptCount: number,
  profiles: string[],
  state: McpServerSummary["state"] = "healthy",
): McpServerSummary => ({
  id,
  name: id,
  uriScheme: id,
  transport: "streamable_http",
  endpoint: `http://${id}-mcp:8799/${id}/mcp`,
  state,
  checkedAt: ago(0),
  capabilities: {
    tools: toolCount > 0,
    resources: resourceCount > 0,
    resourceTemplates: resourceCount > 0,
    resourceSubscriptions: resourceCount > 0,
    prompts: promptCount > 0,
    completions: true,
    tasks: true,
    notifications: true,
  },
  tools: Array.from({ length: toolCount }, (_, index) => `${id}_tool_${index + 1}`),
  compatibilityHelpers: [],
  resources: Array.from({ length: resourceCount }, (_, index) => `${id}://resource/${index + 1}`),
  prompts: Array.from({ length: promptCount }, (_, index) => `${id}-prompt-${index + 1}`),
  requiredScopes: ["operator:use"],
  ownedRoutes: [],
  profiles,
});

export const demoSnapshot: InstallationSnapshot = {
  installation: {
    name: "Veoveo Operations",
    productLabel: "Operations",
    version: "0.1.0",
    offlineMode: false,
    generatedAt: new Date(now).toISOString()
  },
  stream: { cursor: "0" },
  session: {
    displayName: "Mara Chen",
    principalId: "entra#8c19f2",
    tenantId: "field-ops",
    tenantName: "Field Operations",
    availableTenants: [
      { id: "field-ops", name: "Field Operations" },
      { id: "mobility", name: "Mobility Research" }
    ]
  },
  services: [
    { id: "surreal", name: "SurrealDB", kind: "database", state: "healthy", detail: "Control store · RocksDB", latencyMs: 4, checkedAt: ago(0) },
    { id: "gateway", name: "MCP Gateway", kind: "gateway", state: "healthy", detail: "8 profiles active", latencyMs: 12, checkedAt: ago(0) },
    { id: "artifacts", name: "Artifact Plane", kind: "object_store", state: "healthy", detail: "RustFS reachable", latencyMs: 18, checkedAt: ago(0) },
    { id: "recording", name: "Recording Hub", kind: "mcp", state: "degraded", detail: "1 producer reconnecting", latencyMs: 27, checkedAt: ago(1) },
    { id: "otel", name: "Telemetry", kind: "observability", state: "healthy", detail: "Exporter connected", latencyMs: 9, checkedAt: ago(0) }
  ],
  tasks: [
    { id: "019f4d5f-7c31-7e22-962f-910bca693a50", type: "media.generate", server: "media", owner: "mara.chen", state: "waiting", recoveryClass: "webhook_wait", progress: 0.52, createdAt: ago(18), updatedAt: ago(2), message: "Waiting for provider webhook" },
    { id: "019f4d45-f126-7b87-a243-c3d1fc2bdf18", type: "forecast.batch", server: "timeseries", owner: "pilot-agent", state: "running", recoveryClass: "resume", progress: 0.78, createdAt: ago(9), updatedAt: ago(0), message: "Rendering RRD output" },
    { id: "019f4d22-13d6-7acf-a20c-bdd2f476044a", type: "duckdb.execute", server: "duckdb", owner: "mara.chen", state: "failed", recoveryClass: "interrupted_indeterminate", progress: 0.31, createdAt: ago(44), updatedAt: ago(39), message: "Worker interrupted; mutation was not replayed" },
    { id: "019f4cfa-b783-7fab-85f6-4ed88f6c01b2", type: "frames.batch", server: "frames", owner: "pilot-agent", state: "succeeded", recoveryClass: "resume", progress: 1, createdAt: ago(67), updatedAt: ago(63), resultArtifactId: "019f4d01-3d7c-71dd-88fd-348009550aa4" },
    { id: "019f4ce0-1288-7e94-90cc-b3cd97988262", type: "optimization.solve", server: "optimization", owner: "pilot-agent", state: "queued", recoveryClass: "resume", progress: 0, createdAt: ago(3), updatedAt: ago(3), message: "Queued for worker" }
  ],
  artifacts: [
    { id: "019f4d01-3d7c-71dd-88fd-348009550aa4", filename: "survey-transform.rrd", mediaType: "application/vnd.rerun.rrd", byteLength: 18_450_240, owner: "pilot-agent", taskId: "019f4cfa-b783-7fab-85f6-4ed88f6c01b2", classification: "internal", labels: ["field-data"], releaseState: "releasable", authorizedGrants: 4, activeLinks: 1, grants: [], shareLinks: [], retentionExpiresAt: new Date(now + 21 * 86400_000).toISOString(), createdAt: ago(63) },
    { id: "019f4cbb-8aa7-7664-ac33-a247570358d5", filename: "traffic-plan.parquet", mediaType: "application/vnd.apache.parquet", byteLength: 2_103_884, owner: "mara.chen", classification: "internal", labels: ["mobility"], releaseState: "private", authorizedGrants: 2, activeLinks: 0, grants: [], shareLinks: [], createdAt: ago(148) },
    { id: "019f4c72-9749-7146-8c35-e3e7d9a41640", filename: "crossing-preview.png", mediaType: "image/png", byteLength: 7_842_944, owner: "mara.chen", classification: "public", labels: [], releaseState: "released", authorizedGrants: 1, activeLinks: 2, grants: [], shareLinks: [], retentionExpiresAt: new Date(now + 26 * 86400_000).toISOString(), createdAt: ago(205) },
    { id: "019f4c41-428a-77aa-bb6e-d4ac105182c3", filename: "vehicle-counts.csv", mediaType: "text/csv", byteLength: 184_302, owner: "pilot-agent", classification: "restricted", labels: ["customer-a", "location"], releaseState: "private", authorizedGrants: 3, activeLinks: 0, grants: [], shareLinks: [], createdAt: ago(310) }
  ],
  agents: [
    { id: "pilot", name: "Survey Pilot", profile: "field-operator", state: "running", pendingWakes: 1, lastEpisodeAt: ago(1), detail: "Episode 184 · tool execution" },
    { id: "traffic-analyst", name: "Traffic Analyst", profile: "mobility-analyst", state: "waiting", pendingWakes: 0, lastEpisodeAt: ago(22), detail: "Waiting on sim://congestion" },
    { id: "catalog-curator", name: "Catalog Curator", profile: "data-steward", state: "idle", pendingWakes: 2, lastEpisodeAt: ago(91), detail: "Next heartbeat in 4m" }
  ],
  recordings: [
    { id: "019f4d5b-b0ba-7d13-8b25-f09df48f3a83", application: "traffic-world", recordingKey: "luxembourg-morning", state: "live", segmentCount: 8, playableSegmentCount: 7, playableByteLength: 1_940_402_118, startedAt: ago(82), lastDataAt: ago(0) },
    { id: "019f4b88-0d0c-7dc7-a74d-cb8f197c80e8", application: "survey-drone-7", recordingKey: "ridge-east", state: "sealed", segmentCount: 12, playableSegmentCount: 12, playableByteLength: 4_229_005_702, startedAt: ago(1440), lastDataAt: ago(1300), endedAt: ago(1300), sealedAt: ago(1290) },
    { id: "019f4b12-a299-79ae-9817-a24d2f65cd6a", application: "pilot-agent", recordingKey: "episodes-2026-07-09", state: "sealed", segmentCount: 4, playableSegmentCount: 4, playableByteLength: 229_014_806, startedAt: ago(640), lastDataAt: ago(90), endedAt: ago(90), sealedAt: ago(80) }
  ],
  servers: [
    mcpServer("media", 4, 6, 2, ["field-operator", "admin"]),
    mcpServer("duckdb", 8, 5, 2, ["field-operator", "mobility-analyst", "admin"]),
    mcpServer("frames", 3, 5, 3, ["field-operator", "admin"]),
    mcpServer("map", 13, 8, 3, ["field-operator", "admin"]),
    mcpServer("recording", 5, 9, 2, ["field-operator", "mobility-analyst", "admin"], "degraded")
  ],
  policies: [
    { id: "operations", name: "Operations access", revision: 12, state: "active", rules: 18, updatedAt: ago(46) },
    { id: "artifact-release", name: "Artifact release", revision: 6, state: "active", rules: 9, updatedAt: ago(280) },
    { id: "recording-retention", name: "Recording retention", revision: 3, state: "draft", rules: 5, updatedAt: ago(15) }
  ],
  audit: [
    { id: "a-10492", occurredAt: ago(1), actor: "pilot-agent", action: "tools/call", resource: "frames:batch_transform", outcome: "allowed", sourceIp: "10.24.0.18", traceId: "4a0f92cf" },
    { id: "a-10491", occurredAt: ago(2), actor: "mara.chen", action: "artifact/share_link.create", resource: "019f4d01-3d7c-71dd-88fd-348009550aa4", outcome: "allowed", sourceIp: "10.24.1.42", traceId: "2ed1c407" },
    { id: "a-10490", occurredAt: ago(4), actor: "traffic-analyst", action: "tools/call", resource: "duckdb:execute", outcome: "denied", sourceIp: "10.24.0.21", traceId: "c9f8a83d" },
    { id: "a-10489", occurredAt: ago(7), actor: "mara.chen", action: "admin/policy.update", resource: "recording-retention:3", outcome: "allowed", sourceIp: "10.24.1.42", traceId: "8113b599" }
  ]
};
