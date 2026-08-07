import assert from "node:assert/strict";
import test from "node:test";

import {
  AuthenticationRequiredError,
  consoleLoginPath,
  redirectToLogin,
} from "./auth.ts";

test("parallel authentication failures preserve one exact Console route", () => {
  const navigations: string[] = [];
  const navigate = (path: string) => navigations.push(path);
  const returnPath = "/console/#/apps/uav-sim/live.html";

  assert.equal(redirectToLogin(navigate, returnPath), true);
  assert.equal(redirectToLogin(navigate, returnPath), false);
  assert.deepEqual(navigations, [consoleLoginPath(returnPath)]);
  assert.equal(
    consoleLoginPath(returnPath),
    "/auth/login?return_to=%2Fconsole%2F%23%2Fapps%2Fuav-sim%2Flive.html",
  );
  assert.equal(new AuthenticationRequiredError().message, "Authentication required");
});
