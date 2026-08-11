import assert from "node:assert/strict";
import test from "node:test";

import { isFullBleedApp } from "./appPresentation.ts";

test("only an explicit border opt-out selects full-bleed presentation", () => {
  assert.equal(isFullBleedApp({ prefersBorder: false }), true);
  assert.equal(isFullBleedApp({ prefersBorder: true }), false);
  assert.equal(isFullBleedApp({}), false);
  assert.equal(isFullBleedApp(undefined), false);
});
