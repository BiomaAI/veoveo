import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { McpServer } from "@modelcontextprotocol/server";
import {
  THEME_PRESETS,
  cjsAllTemplateDefs,
  ecAllTemplateDefs,
  listThemePresets,
  vlAllTemplateDefs,
} from "flint-chart";
import {
  assembleForBackend,
  renderChart,
  resolveDataSource,
  stripPrivateKeys,
} from "flint-chart-mcp/render";
import { z } from "zod/v4";

const PACKAGE_ROOT = dirname(fileURLToPath(import.meta.url));
const VERSION = JSON.parse(readFileSync(join(PACKAGE_ROOT, "package.json"), "utf8")).version;
const APP_EXTENSION = "io.modelcontextprotocol/ui";
const APP_MIME_TYPE = "text/html;profile=mcp-app";
const CHART_VIEW_URI = "ui://flint-chart/chart-view.html";
const AGENT_SKILL_URI = "flint://agent-skill";
const THEME_SKILL_URI = "flint://theme-skill";
const SUPPORTED_BACKENDS = ["vegalite", "echarts", "chartjs"];
const REGISTRY = {
  vegalite: vlAllTemplateDefs,
  echarts: ecAllTemplateDefs,
  chartjs: cjsAllTemplateDefs,
};

function asset(name) {
  return readFileSync(join(PACKAGE_ROOT, "assets", name), "utf8");
}

function errorResult(error) {
  const message = error instanceof Error ? error.message : String(error);
  return { content: [{ type: "text", text: `Error: ${message}` }], isError: true };
}

function jsonResult(value) {
  return {
    content: [{ type: "text", text: JSON.stringify(value, null, 2) }],
    structuredContent: value,
  };
}

function dataSchema(disableFileReference) {
  return z.object({
    values: z.array(z.record(z.string(), z.unknown())).optional(),
    url: z.string().optional().describe(
      disableFileReference
        ? "Disabled; provide inline data.values."
        : "Local JSON, CSV, or TSV path. Remote URLs are rejected.",
    ),
  });
}

const chartSpecSchema = z.object({
  chartType: z.string(),
  title: z.string().optional(),
  subtitle: z.string().optional(),
  encodings: z.record(z.string(), z.unknown()),
  baseSize: z.object({ width: z.number(), height: z.number() }).optional(),
  canvasSize: z.object({ width: z.number(), height: z.number() }).optional(),
  chartProperties: z.record(z.string(), z.unknown()).optional(),
});

function assemblySchema(disableFileReference) {
  return z.object({
    data: dataSchema(disableFileReference),
    semantic_types: z.record(z.string(), z.unknown()).optional(),
    chart_spec: chartSpecSchema,
    theme_spec: z.union([z.string(), z.record(z.string(), z.unknown())]).optional(),
    options: z.record(z.string(), z.unknown()).optional(),
    field_display_names: z.record(z.string(), z.string()).optional(),
  });
}

function assemblyInput(args) {
  return {
    data: args.data,
    semantic_types: args.semantic_types,
    chart_spec: args.chart_spec,
    theme_spec: args.theme_spec,
    options: args.options,
    field_display_names: args.field_display_names,
  };
}

function listChartTypes(backend) {
  const backends = backend ? [backend] : SUPPORTED_BACKENDS;
  return {
    backends: backends.map((name) => {
      const chartTypes = (REGISTRY[name] ?? [])
        .map((definition) => ({
          chartType: definition.chart,
          channels: definition.channels ?? [],
        }))
        .sort((left, right) => left.chartType.localeCompare(right.chartType));
      return { backend: name, count: chartTypes.length, chartTypes };
    }),
  };
}

function listThemes(id) {
  if (!id) return { themes: listThemePresets() };
  const preset = THEME_PRESETS[id];
  if (!preset) throw new Error(`Unknown theme \`${id}\`.`);
  const { spec: _spec, icon: _icon, ...guidance } = preset;
  return guidance;
}

function compile(input, backend, options) {
  const assembled = assembleForBackend(backend, input, options);
  return {
    backend,
    spec: stripPrivateKeys(assembled.spec),
    warnings: assembled.warnings,
    computedSize:
      assembled.width !== undefined && assembled.height !== undefined
        ? { width: assembled.width, height: assembled.height }
        : undefined,
  };
}

function validate(input, backend, options) {
  try {
    const compiled = compile(input, backend, options);
    return {
      backend,
      chartType: input.chart_spec.chartType,
      valid: true,
      warnings: compiled.warnings,
      errors: [],
      computedSize: compiled.computedSize,
    };
  } catch (error) {
    return {
      backend,
      chartType: input?.chart_spec?.chartType ?? "unknown",
      valid: false,
      warnings: [],
      errors: [{
        severity: "error",
        code: "assembly_failed",
        message: error instanceof Error ? error.message : String(error),
      }],
    };
  }
}

function registerResource(server, name, uri, title, mimeType, text, meta) {
  server.registerResource(
    name,
    uri,
    { title, mimeType },
    async () => ({
      contents: [{ uri, mimeType, text, ...(meta ? { _meta: meta } : {}) }],
    }),
  );
}

export function createServer({ disableFileReference = false } = {}) {
  const dataOptions = { disableFileReference };
  const inputSchema = assemblySchema(disableFileReference);
  const backend = z.enum(SUPPORTED_BACKENDS);
  const server = new McpServer(
    { name: "charts", version: VERSION },
    {
      instructions:
        "Compile, validate, and render Flint chart specifications. Prefer create_chart_view when the client supports MCP Apps.",
      capabilities: { extensions: { [APP_EXTENSION]: {} } },
      cacheHints: {
        "server/discover": { ttlMs: 30000, scope: "private" },
        "tools/list": { ttlMs: 30000, scope: "private" },
        "prompts/list": { ttlMs: 30000, scope: "private" },
        "resources/list": { ttlMs: 30000, scope: "private" },
        "resources/templates/list": { ttlMs: 30000, scope: "private" },
        "resources/read": { ttlMs: 30000, scope: "private" },
      },
    },
  );

  server.registerTool(
    "render_chart",
    {
      title: "Render chart",
      description: "Render a Flint chart to PNG or SVG.",
      inputSchema: inputSchema.extend({
        backend,
        format: z.enum(["png", "svg"]).optional(),
        scale: z.number().min(0.5).max(4).optional(),
        background: z.string().optional(),
      }),
    },
    async (args) => {
      try {
        const result = await renderChart(assemblyInput(args), args.backend, {
          format: args.format,
          scale: args.scale,
          background: args.background,
          ...dataOptions,
        });
        const content = result.format === "svg"
          ? [{ type: "text", text: result.svg }]
          : [{ type: "image", data: result.base64, mimeType: result.mimeType }];
        content.push({
          type: "text",
          text: `${result.backend} · ${result.format} · ${result.width}×${result.height}px`,
        });
        return { content };
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  server.registerTool(
    "compile_chart",
    {
      title: "Compile chart spec",
      description: "Compile a Flint chart to a backend-native specification.",
      inputSchema: inputSchema.extend({ backend }),
    },
    async (args) => {
      try {
        return jsonResult(compile(assemblyInput(args), args.backend, dataOptions));
      } catch (error) {
        return errorResult(error);
      }
    },
  );
  server.registerTool(
    "validate_chart",
    {
      title: "Validate chart spec",
      description: "Validate a Flint chart without rendering it.",
      inputSchema: inputSchema.extend({ backend }),
    },
    async (args) => jsonResult(validate(assemblyInput(args), args.backend, dataOptions)),
  );
  server.registerTool(
    "list_chart_types",
    {
      title: "List chart types",
      description: "List chart templates and their channels.",
      inputSchema: z.object({ backend: backend.optional() }),
    },
    async (args) => jsonResult(listChartTypes(args.backend)),
  );
  server.registerTool(
    "list_themes",
    {
      title: "List themes",
      description: "List Flint visual theme presets.",
      inputSchema: z.object({ id: z.string().optional() }),
    },
    async (args) => {
      try {
        return jsonResult(listThemes(args.id));
      } catch (error) {
        return errorResult(error);
      }
    },
  );
  server.registerTool(
    "create_chart_view",
    {
      title: "Create interactive chart view",
      description: "Open an interactive MCP App chart view.",
      inputSchema,
      _meta: { ui: { resourceUri: CHART_VIEW_URI } },
    },
    async (args) => {
      try {
        const input = resolveDataSource(assemblyInput(args), dataOptions);
        const summary = validate(input, "vegalite", dataOptions);
        return {
          content: [{
            type: "text",
            text: summary.valid
              ? `Interactive chart view ready: ${summary.chartType}`
              : `Chart spec has errors: ${summary.errors.map((error) => error.message).join("; ")}`,
          }],
          structuredContent: { input },
          ...(summary.valid ? {} : { isError: true }),
        };
      } catch (error) {
        return errorResult(error);
      }
    },
  );

  registerResource(
    server,
    "chart-view",
    CHART_VIEW_URI,
    "Interactive chart view",
    APP_MIME_TYPE,
    asset("flint-app.html"),
    { ui: { permissions: { clipboardWrite: {} } } },
  );
  registerResource(
    server,
    "chart-types",
    "flint://chart-types",
    "Flint chart types",
    "application/json",
    JSON.stringify(listChartTypes(), null, 2),
  );
  registerResource(
    server,
    "agent-skill",
    AGENT_SKILL_URI,
    "Flint chart-author skill",
    "text/markdown",
    asset("flint-chart-author.SKILL.md"),
  );
  registerResource(
    server,
    "theme-skill",
    THEME_SKILL_URI,
    "Flint theme-author skill",
    "text/markdown",
    asset("flint-theme-author.SKILL.md"),
  );
  server.registerPrompt(
    "author_flint_chart",
    { title: "Author a Flint chart" },
    async () => ({
      messages: [{
        role: "user",
        content: {
          type: "resource",
          resource: {
            uri: AGENT_SKILL_URI,
            mimeType: "text/markdown",
            text: asset("flint-chart-author.SKILL.md"),
          },
        },
      }],
    }),
  );
  return server;
}
