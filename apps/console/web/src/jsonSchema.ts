import { CfWorkerJsonSchemaValidator } from "@modelcontextprotocol/client/validators/cf-worker";

export type SchemaParseResult =
  | { success: true; data: unknown }
  | { success: false; error: Error };

export interface GeneratedSchemaValidator {
  parse(value: unknown): unknown;
  safeParse(value: unknown): SchemaParseResult;
}

const provider = new CfWorkerJsonSchemaValidator();

/** Compile the repository's generated JSON Schema profile without mutating it. */
export function compileGeneratedSchema(schema: object): GeneratedSchemaValidator {
  // Interpret the generated schema directly. This provider supports 2020-12
  // composition and references without code generation, as required by our CSP.
  const validate = provider.getValidator(schema);
  const safeParse = (value: unknown): SchemaParseResult => {
    const result = validate(value);
    return result.valid
      ? { success: true, data: result.data }
      : { success: false, error: new Error(result.errorMessage) };
  };
  return {
    safeParse,
    parse(value) {
      const result = safeParse(value);
      if (!result.success) throw result.error;
      return result.data;
    },
  };
}
