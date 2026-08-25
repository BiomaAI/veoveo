import type { AppCatalog } from "../types";

export function attachAppCatalogEvents(
  source: Pick<EventSource, "addEventListener" | "close">,
  receive: (catalog: AppCatalog) => void,
): () => void {
  source.addEventListener("catalog", (event) => {
    receive(JSON.parse((event as MessageEvent<string>).data) as AppCatalog);
  });
  return () => source.close();
}
