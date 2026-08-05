import assert from "node:assert/strict";
import test from "node:test";

import { resetRerunEmbeddedViewerState } from "./rerunEmbeddedState.ts";

test("removes only Rerun 0.35 embedded viewer state", () => {
  const removed: string[] = [];
  resetRerunEmbeddedViewerState({
    removeItem(key: string) {
      removed.push(key);
    },
  } as Storage);
  assert.deepEqual(removed, ["app", "egui_memory_ron", "rerun.redap_token"]);
});
