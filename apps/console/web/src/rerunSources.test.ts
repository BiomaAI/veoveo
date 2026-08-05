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
  "https://console.example/console/api/recordings/019fab95-e208-7901-9db7-77c8444652db/live/rrd-stream";

function live(route = liveRoute): GovernedRerunReceiver {
  return { kind: "live", route };
}

test("active recording selects one incremental Rerun channel", () => {
  assert.deepEqual(selectExclusiveRerunPlaybackReceiver("live", archive, liveRoute), {
    mode: "live",
    receiver: live(),
  });
});

test("history mode selects one immutable archive receiver", () => {
  assert.deepEqual(selectExclusiveRerunPlaybackReceiver("archive", archive, liveRoute), {
    mode: "archive",
    receiver: { kind: "archive", archive },
  });
});

test("requested mode falls back to the available receiver", () => {
  assert.deepEqual(selectExclusiveRerunPlaybackReceiver("live", archive, undefined), {
    mode: "archive",
    receiver: { kind: "archive", archive },
  });
  assert.deepEqual(selectExclusiveRerunPlaybackReceiver("archive", undefined, liveRoute), {
    mode: "live",
    receiver: live(),
  });
});

test("opens the producer Blueprint before the archive receiver", () => {
  const transition = planRerunSourceTransition(
    {},
    {
      redapToken: "token-a",
      blueprintUrl: "https://console.example/blueprints/1.rrd",
      receiver: { kind: "archive", archive },
    }
  );
  assert.equal(transition.blueprintUrlToOpen, "https://console.example/blueprints/1.rrd");
  assert.equal(transition.archiveUrlToOpen, "rerun://archive");
  assert.equal(transition.liveRouteToOpen, undefined);
});

test("replaces a Blueprint without changing its receiver", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      receiver: live(),
      blueprintUrl: "https://console.example/blueprints/1.rrd",
    },
    {
      redapToken: "token-a",
      receiver: live(),
      blueprintUrl: "https://console.example/blueprints/2.rrd",
    }
  );
  assert.deepEqual(transition.urlsToCloseBeforeOpen, [
    "https://console.example/blueprints/1.rrd",
  ]);
  assert.equal(transition.closeLiveConnection, false);
  assert.equal(transition.liveRouteToOpen, undefined);
});

test("route replacement reconnects the HTTP source but keeps one Rerun channel", () => {
  const replacement = liveRoute.replace(
    "019fab95-e208-7901-9db7-77c8444652db",
    "019fab95-e208-7901-9db7-77c8444652dc"
  );
  const transition = planRerunSourceTransition(
    { redapToken: "token-a", receiver: live() },
    { redapToken: "token-a", receiver: live(replacement) }
  );
  assert.equal(transition.closeLiveConnection, true);
  assert.equal(transition.liveRouteToOpen, replacement);
  assert.deepEqual(transition.urlsToCloseBeforeOpen, []);
});

test("session renewal does not churn a live channel", () => {
  const transition = planRerunSourceTransition(
    { redapToken: "token-a", receiver: live() },
    { redapToken: "token-b", receiver: live() }
  );
  assert.equal(transition.credentialsChanged, true);
  assert.equal(transition.closeLiveConnection, false);
  assert.equal(transition.liveRouteToOpen, undefined);
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
