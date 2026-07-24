import assert from "node:assert/strict";
import test from "node:test";

import { AuthenticationRequiredError, redirectToLogin } from "./auth.ts";

test("parallel authentication failures start exactly one login navigation", () => {
  const navigations: string[] = [];
  const navigate = (path: string) => navigations.push(path);

  assert.equal(redirectToLogin(navigate), true);
  assert.equal(redirectToLogin(navigate), false);
  assert.deepEqual(navigations, ["/auth/login"]);
  assert.equal(new AuthenticationRequiredError().message, "Authentication required");
});
