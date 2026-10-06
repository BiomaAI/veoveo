import { compileGeneratedSchema } from "./jsonSchema.ts";
import computerSchema from "./generated/computers.schema.json" with { type: "json" };
import consoleSchema from "./generated/console.schema.json" with { type: "json" };
import auditSchema from "./generated/audit.schema.json" with { type: "json" };
import type { AuditReaderApi } from "./generated/audit.ts";
import type { ComputerId, ComputersApi } from "./generated/computers.ts";
import type { ConsoleApi } from "./generated/console.ts";

// The cast joins generated interfaces to the very schema which generated them.
// No caller supplies a schema, and malformed wire values never become domain objects.
function parser<T>(schema: object): (value: unknown) => T {
  const validator = compileGeneratedSchema(schema);
  return (value) => validator.parse(value) as T;
}
const consoleParsers = new Map<keyof ConsoleApi, (value: unknown) => unknown>();
export function parseConsole<K extends keyof ConsoleApi>(kind: K, value: unknown): ConsoleApi[K] {
  let parse = consoleParsers.get(kind);
  if (!parse) {
    parse = parser({ $schema: consoleSchema.$schema, $defs: consoleSchema.$defs, ...consoleSchema.properties[kind] });
    consoleParsers.set(kind, parse);
  }
  return parse(value) as ConsoleApi[K];
}
export const parseConsoleBootstrap = (value: unknown) => parseConsole("bootstrap", value);
export const parseComputerId = parser<ComputerId>(computerSchema.$defs.ComputerId);
const auditParsers = new Map<keyof AuditReaderApi, (value: unknown) => unknown>();
export function parseAudit<K extends keyof AuditReaderApi>(kind: K, value: unknown): AuditReaderApi[K] {
  let parse = auditParsers.get(kind);
  if (!parse) {
    parse = parser({
      $schema: auditSchema.$schema,
      $defs: auditSchema.$defs,
      ...auditSchema.properties[kind],
    });
    auditParsers.set(kind, parse);
  }
  return parse(value) as AuditReaderApi[K];
}
const computerParsers = new Map<keyof ComputersApi, (value: unknown) => unknown>();
export function parseComputer<K extends keyof ComputersApi>(
  kind: K,
  value: unknown,
): ComputersApi[K] {
  let parse = computerParsers.get(kind);
  if (!parse) {
    parse = parser({
      $schema: computerSchema.$schema,
      $defs: computerSchema.$defs,
      ...computerSchema.properties[kind],
    });
    computerParsers.set(kind, parse);
  }
  return parse(value) as ComputersApi[K];
}
