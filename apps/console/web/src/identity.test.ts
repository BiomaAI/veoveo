import assert from "node:assert/strict";
import test from "node:test";
import type { IdentityDirectory } from "./identity.ts";
import { compactIdentityLabel, identityLabel } from "./identity.ts";

const oidcId = "https://login.microsoftonline.com/tenant/v2.0#624e4793-e783-43c6-af47-4be53d7b9b92";
const directory: IdentityDirectory = {
  session: {
    displayName: "Alex Rozgo",
    principalId: oidcId,
    actorId: oidcId,
    tenantId: "enterprise",
    tenantName: "Enterprise",
    workContext: "operations",
    workContextTitle: "Operations",
    membership: "owner",
    invocationMode: "direct",
    availableTenants: [],
  },
  principals: [
    { id: oidcId, displayName: "Alex Rozgo" },
    { id: "https://issuer.example#second-user", displayName: "Second User" },
  ],
};

test("renders the authenticated OIDC principal as its trusted display name", () => {
  assert.equal(identityLabel(oidcId, directory), "Alex Rozgo");
});

test("resolves other known principals through the generic directory", () => {
  assert.equal(identityLabel("https://issuer.example#second-user", directory), "Second User");
});

test("never exposes an unknown OIDC URL as the visible fallback", () => {
  assert.equal(compactIdentityLabel(oidcId), "Principal 624e4793");
  assert.equal(identityLabel("https://issuer.example#rozgo", directory), "rozgo");
});
