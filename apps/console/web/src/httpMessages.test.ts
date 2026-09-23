import assert from "node:assert/strict";
import test from "node:test";

import { httpErrorMessage } from "./httpMessages.ts";
import { invocationModeLabel, recoveryClassLabel, statusLabel } from "./labels.ts";

test("HTTP failures become actionable messages without status codes", () => {
  const subject = { action: "load access requests", thing: "This request" };
  assert.match(httpErrorMessage(401, subject), /Sign in again/);
  assert.equal(
    httpErrorMessage(403, subject),
    "You don't have permission to load access requests. Ask an administrator for access.",
  );
  assert.equal(httpErrorMessage(404, subject), "This request was not found. It may have been removed.");
  for (const status of [500, 502, 503]) {
    const message = httpErrorMessage(status, subject);
    assert.match(message, /^Veoveo couldn't load access requests\. Try again/);
    assert.equal(message.includes(String(status)), false);
  }
});

test("enum values render as readable labels", () => {
  assert.equal(recoveryClassLabel("interrupted_indeterminate"), "Not retried if interrupted");
  assert.equal(statusLabel("budget_terminated"), "stopped at budget limit");
  assert.equal(statusLabel("cancel_requested"), "cancelling");
  assert.equal(statusLabel("running"), "running");
  assert.equal(invocationModeLabel("delegated"), "On behalf of a person");
});
