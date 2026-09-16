// MCP Apps 2026-01-26 host adapter for native MCP 2026-07-28 result envelopes.
// The SDK types own tool/resource content; open native fields remain intact.
import { z } from "zod";
import type { AppCatalog } from "../../console/web/src/types.ts";
import type { AppToolResult, TaskDetailResult, TaskAckResult, ReadResourceResult } from "../../console/web/src/apps/protocol.ts";

const object = z.record(z.string(), z.unknown());
const tool = z.object({ name: z.string(), title: z.string().optional(), description: z.string().optional(), inputSchema: object });
export const appCatalog: z.ZodType<AppCatalog> = z.object({
  apps: z.array(z.object({ server: z.string(), resourceUri: z.string().startsWith("ui://"), standalonePath: z.string(), name: z.string(), title: z.string().optional(), description: z.string().optional(), icons: z.array(z.string()).optional(), prefersBorder: z.boolean().optional(), tools: z.array(tool),
    resourceDependencies: z.array(z.object({ app_resource: z.string(), server: z.string(), scheme: z.string(), uri_prefix: z.string(), required_scope: z.string(), operations: z.array(z.enum(["read", "subscribe"])), data_labels: z.array(z.string()).optional() })),
    toolDependencies: z.array(z.object({ app_resource: z.string(), server: z.string(), required_scope: z.string(), data_labels: z.array(z.string()).optional(), tools: z.array(z.object({ name: z.string(), target_tool: z.string() })) })),
    agentMessageTargets: z.array(z.string()),
  })),
  degradations: z.array(z.object({ server: z.string(), surface: z.enum(["resources", "resource_templates", "tools"]), code: z.literal("upstream_unavailable") })),
});
const requests = z.record(z.string(), z.object({ method: z.string(), params: object.optional() }).passthrough());
const seed = z.object({ taskId: z.string().min(1).max(4096), status: z.enum(["working", "input_required", "completed", "failed", "cancelled"]), statusMessage: z.string().optional(), createdAt: z.string(), lastUpdatedAt: z.string(), ttlMs: z.number().nullable(), pollIntervalMs: z.number().optional() }).passthrough();
// Native result fields remain open for SDK-owned content and extension metadata.
export const appResult = z.discriminatedUnion("resultType", [
  seed.extend({ resultType: z.literal("task") }),
  z.object({ resultType: z.literal("input_required"), inputRequests: requests.optional(), requestState: z.string().optional() }).passthrough(),
  z.object({ resultType: z.literal("complete"), content: z.array(object), isError: z.boolean().optional(), structuredContent: object.optional() }).passthrough(),
]) as unknown as z.ZodType<AppToolResult>;
export const taskDetail = seed.extend({ resultType: z.literal("complete"), inputRequests: requests.optional(), result: object.optional(), error: object.optional() }) as unknown as z.ZodType<TaskDetailResult>;
export const taskAck: z.ZodType<TaskAckResult> = z.object({ resultType: z.literal("complete") }).passthrough();
export const resourceResult = z.object({ contents: z.array(z.union([
  z.object({ uri: z.string(), text: z.string(), mimeType: z.string().optional() }).passthrough(),
  z.object({ uri: z.string(), blob: z.string(), mimeType: z.string().optional() }).passthrough(),
])) }).passthrough() as z.ZodType<ReadResourceResult>;
