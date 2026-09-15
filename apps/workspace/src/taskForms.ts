import { z } from "zod";

const shape = z.object({ type: z.literal("object"), properties: z.record(z.string(), z.unknown()), required: z.array(z.string()).optional() });
const primitive = z.looseObject({ type: z.enum(["string", "number", "integer", "boolean", "array"]).optional(), title: z.string().optional(), description: z.string().optional(),
  minimum: z.number().optional(), maximum: z.number().optional(), minLength: z.number().optional(), maxLength: z.number().optional(),
  minItems: z.number().optional(), maxItems: z.number().optional(), format: z.string().optional(), default: z.unknown().optional() });
export type FieldValue = string | number | boolean | string[];
export interface FormField {
  key: string; label: string; description?: string; required: boolean;
  kind: "text" | "number" | "integer" | "boolean" | "select" | "multi";
  choices?: { value: string; label: string }[]; minimum?: number; maximum?: number;
  minLength?: number; maxLength?: number; minItems?: number; maxItems?: number; format?: string; default?: FieldValue;
}
function choices(value: unknown): { value: string; label: string }[] | undefined {
  if (!value || typeof value !== "object") return undefined;
  const schema = value as Record<string, unknown>;
  const values = z.array(z.string()).safeParse(schema.enum);
  if (values.success) {
    const names = z.array(z.string()).safeParse(schema.enumNames);
    return values.data.map((value, index) => ({ value, label: names.success ? names.data[index] ?? value : value }));
  }
  const titled = z.array(z.object({ const: z.string(), title: z.string().optional() })).safeParse(schema.oneOf ?? schema.anyOf);
  return titled.success ? titled.data.map(value => ({ value: value.const, label: value.title ?? value.const })) : undefined;
}
export function formFields(schema: unknown): FormField[] {
  const object = shape.parse(schema);
  if (Object.keys(object.properties).length > 64) throw new Error("This form has too many fields to display.");
  return Object.entries(object.properties).map(([key, value]) => {
    const field = primitive.parse(value);
    const options = choices(value);
    const multi = field.type === "array" ? choices(field.items) : undefined;
    const kind = multi ? "multi" : options ? "select" : field.type === "string" ? "text" : field.type;
    if (!kind || kind === "array") throw new Error("This form uses a field this client cannot display.");
    const initial = z.union([z.string(), z.number(), z.boolean(), z.array(z.string())]).safeParse(field.default);
    return { ...field, key, label: field.title ?? key.replaceAll("_", " "), required: object.required?.includes(key) ?? false,
      kind, choices: multi ?? options, default: initial.success ? initial.data : undefined };
  });
}
export function readForm(fields: FormField[], form: FormData, prefix: string): Record<string, FieldValue> {
  const result: Record<string, FieldValue> = {};
  fields.forEach((field, index) => {
    const key = `${prefix}-${index}`;
    if (field.kind === "boolean") { result[field.key] = form.get(key) === "on"; return; }
    if (field.kind === "multi") {
      const values = form.getAll(key).map(String);
      if ((field.minItems !== undefined && values.length < field.minItems) || (field.maxItems !== undefined && values.length > field.maxItems)
          || values.some(value => !field.choices?.some(choice => choice.value === value))) throw new Error(`Check the choices for ${field.label}.`);
      if (values.length || field.required) result[field.key] = values;
      return;
    }
    const value = String(form.get(key) ?? "");
    if (!value && !field.required) return;
    if (!value && field.required) throw new Error(`Enter ${field.label}.`);
    if (field.kind === "number" || field.kind === "integer") {
      const number = Number(value);
      if (!Number.isFinite(number) || (field.kind === "integer" && !Number.isInteger(number))
          || (field.minimum !== undefined && number < field.minimum) || (field.maximum !== undefined && number > field.maximum)) throw new Error(`Check ${field.label}.`);
      result[field.key] = number;
    } else {
      if (field.kind === "select" && !field.choices?.some(choice => choice.value === value)) throw new Error(`Choose ${field.label}.`);
      if ((field.minLength !== undefined && [...value].length < field.minLength) || (field.maxLength !== undefined && [...value].length > field.maxLength)) throw new Error(`Check the length of ${field.label}.`);
      result[field.key] = value;
    }
  });
  return result;
}
