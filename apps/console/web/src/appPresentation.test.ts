import assert from "node:assert/strict";
import test from "node:test";

import { isFullBleedApp } from "./appPresentation.ts";

test("Apps use the full workspace unless they explicitly request a border", () => {
  assert.equal(isFullBleedApp({ prefersBorder: false }), true);
  assert.equal(isFullBleedApp({ prefersBorder: true }), false);
  assert.equal(isFullBleedApp({}), true);
  assert.equal(isFullBleedApp(undefined), false);
});
