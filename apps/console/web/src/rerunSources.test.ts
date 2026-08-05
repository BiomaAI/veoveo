import assert from "node:assert/strict";
import test from "node:test";

import {
  planRerunSourceTransition,
  requiresPlaybackCredentialRenewal,
  selectExclusiveRerunPlaybackReceiver,
  type GovernedRerunReceiver,
} from "./rerunSources.ts";

const archive = { uri: "rerun://archive", revision: "revision-a" };
const liveRoute =
  "https://console.example/console/api/recordings/019fab95-e208-7901-9db7-77c8444652db/live/proxy";
const viewerUri = "rerun+https://console.example/proxy";

function live(generation = 0, route = liveRoute): GovernedRerunReceiver {
  return { kind: "live", route, viewerUri, generation };
}

test("active recording selects one native MessageProxy receiver", () => {
  assert.deepEqual(
    selectExclusiveRerunPlaybackReceiver(
      "live",
      archive,
      liveRoute,
      viewerUri
    ),
    { mode: "live", receiver: live() }
  );
});

test("history mode selects one immutable archive receiver", () => {
  assert.deepEqual(
    selectExclusiveRerunPlaybackReceiver(
      "archive",
      archive,
      liveRoute,
      viewerUri
    ),
    { mode: "archive", receiver: { kind: "archive", archive } }
  );
});

test("requested mode falls back to the available receiver", () => {
  assert.deepEqual(
    selectExclusiveRerunPlaybackReceiver(
      "live",
      archive,
      undefined,
      undefined
    ),
    { mode: "archive", receiver: { kind: "archive", archive } }
  );
  assert.deepEqual(
    selectExclusiveRerunPlaybackReceiver(
      "archive",
      undefined,
      liveRoute,
      viewerUri
    ),
    { mode: "live", receiver: live() }
  );
});

test("opens the producer Blueprint before the sole archive receiver", () => {
  const transition = planRerunSourceTransition(
    {},
    {
      redapToken: "token-a",
      blueprintUrl: "https://console.example/blueprints/1.rrd",
      receiver: { kind: "archive", archive },
    }
  );
  assert.equal(
    transition.blueprintUrlToOpen,
    "https://console.example/blueprints/1.rrd"
  );
  assert.equal(transition.receiverUrlToOpen, "rerun://archive");
  assert.deepEqual(transition.urlsToCloseBeforeOpen, []);
});

test("replaces a Blueprint before opening its new revision", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      receiver: { kind: "archive", archive },
      blueprintUrl: "https://console.example/blueprints/1.rrd",
    },
    {
      redapToken: "token-a",
      receiver: { kind: "archive", archive },
      blueprintUrl: "https://console.example/blueprints/2.rrd",
    }
  );
  assert.deepEqual(transition.urlsToCloseBeforeOpen, [
    "https://console.example/blueprints/1.rrd",
  ]);
  assert.equal(
    transition.blueprintUrlToOpen,
    "https://console.example/blueprints/2.rrd"
  );
});

test("switches native MessageProxy live playback to history without overlap", () => {
  const transition = planRerunSourceTransition(
    { redapToken: "token-a", receiver: live() },
    { redapToken: "token-a", receiver: { kind: "archive", archive } }
  );
  assert.deepEqual(transition.urlsToCloseBeforeOpen, [viewerUri]);
  assert.equal(transition.receiverUrlToOpen, "rerun://archive");
});

test("session renewal does not churn a live receiver", () => {
  const transition = planRerunSourceTransition(
    { redapToken: "token-a", receiver: live() },
    { redapToken: "token-b", receiver: live() }
  );
  assert.equal(transition.credentialsChanged, true);
  assert.equal(transition.receiverUrlToOpen, undefined);
  assert.deepEqual(transition.urlsToCloseBeforeOpen, []);
});

test("storage segment rollover does not reopen the recording receiver", () => {
  const transition = planRerunSourceTransition(
    { redapToken: "token-a", receiver: live() },
    { redapToken: "token-a", receiver: live() }
  );
  assert.equal(transition.receiverUrlToOpen, undefined);
  assert.deepEqual(transition.urlsToCloseBeforeOpen, []);
});

test("route replacement and explicit reconnect reopen the one proxy URI", () => {
  for (const desired of [live(0, `${liveRoute}2`), live(1)]) {
    const transition = planRerunSourceTransition(
      { redapToken: "token-a", receiver: live() },
      { redapToken: "token-a", receiver: desired }
    );
    assert.deepEqual(transition.urlsToCloseBeforeOpen, [viewerUri]);
    assert.equal(transition.receiverUrlToOpen, viewerUri);
  }
});

test("only Redap archive playback schedules credential renewal", () => {
  assert.equal(requiresPlaybackCredentialRenewal(live()), false);
  assert.equal(
    requiresPlaybackCredentialRenewal({
      kind: "archive",
      archive: { uri: "rerun://archive", revision: "r1" },
    }),
    true
  );
  assert.equal(requiresPlaybackCredentialRenewal(undefined), false);
});
