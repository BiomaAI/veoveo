import { compileGeneratedSchema } from "./jsonSchema.ts";
import computerSchema from "./generated/computers.schema.json" with { type: "json" };
import consoleSchema from "./generated/console.schema.json" with { type: "json" };
import type { ComputerId, ComputersApi } from "./generated/computers.ts";
import type { ConsoleBootstrap } from "./generated/console.ts";

// The cast joins generated interfaces to the very schema which generated them.
// No caller supplies a schema, and malformed wire values never become domain objects.
function parser<T>(schema: object): (value: unknown) => T {
  const validator = compileGeneratedSchema(schema);
  return (value) => validator.parse(value) as T;
}
export const parseConsoleBootstrap = parser<ConsoleBootstrap>(consoleSchema);
export const parseComputerId = parser<ComputerId>(computerSchema.$defs.ComputerId);
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
