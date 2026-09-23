import { configureBrowserApplication } from "./browserApp.ts";
import assert from "node:assert/strict";
import test from "node:test";

configureBrowserApplication("console");

import {
  AuthenticationRequiredError,
  browserLoginPath,
  redirectToLogin,
} from "./auth.ts";

test("parallel authentication failures preserve one exact Console route", () => {
  const navigations: string[] = [];
  const navigate = (path: string) => navigations.push(path);
  const returnPath = "/console/#/apps/uav-sim/live";

  assert.equal(redirectToLogin(navigate, returnPath), true);
  assert.equal(redirectToLogin(navigate, returnPath), false);
  assert.deepEqual(navigations, [browserLoginPath(returnPath)]);
  assert.equal(
    browserLoginPath(returnPath),
    "/auth/login?return_to=%2Fconsole%2F%23%2Fapps%2Fuav-sim%2Flive",
  );
  assert.equal(new AuthenticationRequiredError().message, "Authentication required");
});
