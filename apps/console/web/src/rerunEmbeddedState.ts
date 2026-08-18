// Rerun 0.36's WebViewer hard-codes persisted standalone-viewer state. An
// embedded governed viewer must not inherit a previously selected Redap server:
// restoring one starts catalog/watch traffic before any recording is opened.
// Producer Blueprints remain the layout authority after this reset.
const RERUN_EMBEDDED_STORAGE_KEYS = ["app", "egui_memory_ron", "rerun.redap_token"];

export function resetRerunEmbeddedViewerState(storage: Storage = window.localStorage): void {
  for (const key of RERUN_EMBEDDED_STORAGE_KEYS) storage.removeItem(key);
}
