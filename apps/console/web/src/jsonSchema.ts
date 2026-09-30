import { z } from "zod";

/** Compile the repository's generated JSON Schema profile without mutating it. */
export function compileGeneratedSchema(schema: object): z.ZodType {
  // Zod's reference lookup treats a named `false` definition as missing. Both
  // boolean schemas have equivalent object forms supported by its converter.
  const document = schema as { $defs?: Record<string, object | boolean> };
  const normalized: object = document.$defs ? {
    ...schema,
    $defs: Object.fromEntries(Object.entries(document.$defs).map(([name, definition]) => [
      name,
      definition === false ? { not: {} } : definition === true ? {} : definition,
    ])),
  } : schema;
  return z.fromJSONSchema(normalized as Parameters<typeof z.fromJSONSchema>[0]);
}
