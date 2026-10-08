import schema from "../generated/app-catalog.schema.json" with { type: "json" };
import { compileGeneratedSchema } from "../jsonSchema.ts";
import type { AppCatalog } from "../types";

const validator = compileGeneratedSchema(schema);

export function attachAppCatalogEvents(
  source: Pick<EventSource, "addEventListener" | "close">,
  receive: (catalog: AppCatalog) => void,
): () => void {
  source.addEventListener("catalog", (event) => {
    let value: unknown;
    try {
      value = JSON.parse((event as MessageEvent<string>).data);
    } catch {
      throw new Error("Invalid App catalog event");
    }
    const admitted = validator.safeParse(value);
    if (!admitted.success) throw new Error("Invalid App catalog event");
    // The owner schema generated AppCatalog; admission precedes delivery.
    receive(admitted.data as AppCatalog);
  });
  return () => source.close();
}
