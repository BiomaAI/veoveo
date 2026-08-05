import assert from "node:assert/strict";
import test from "node:test";

import {
  planRerunSourceTransition,
  requiresPlaybackCredentialRenewal,
  selectExclusiveRerunPlaybackReceiver,
} from "./rerunSources.ts";

const archive = { uri: "rerun://archive", revision: "revision-a" };

test("active recording defaults to one live receiver", () => {
  const selected = selectExclusiveRerunPlaybackReceiver(
    "live",
    archive,
    "https://console.example/live.rrd"
  );

  assert.deepEqual(selected, {
    mode: "live",
    receiver: {
      kind: "live",
      url: "https://console.example/live.rrd",
      generation: 0,
    },
  });
});

test("history mode selects one immutable archive receiver", () => {
  const selected = selectExclusiveRerunPlaybackReceiver(
    "archive",
    archive,
    "https://console.example/live.rrd"
  );

  assert.deepEqual(selected, {
    mode: "archive",
    receiver: { kind: "archive", archive },
  });
});

test("requested playback mode falls back to the available receiver", () => {
  assert.deepEqual(
    selectExclusiveRerunPlaybackReceiver("live", archive, undefined),
    { mode: "archive", receiver: { kind: "archive", archive } }
  );
  assert.deepEqual(
    selectExclusiveRerunPlaybackReceiver(
      "archive",
      undefined,
      "https://console.example/live.rrd"
    ),
    {
      mode: "live",
      receiver: {
        kind: "live",
        url: "https://console.example/live.rrd",
        generation: 0,
      },
    }
  );
});

test("opens the producer Blueprint before the sole recording receiver", () => {
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

test("replaces a Blueprint by closing its old store before opening the revision", () => {
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
  assert.equal(transition.receiverUrlToOpen, undefined);
});

test("switches the native live RRD receiver to history without overlap", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      receiver: { kind: "live", url: "live-1", generation: 0 },
    },
    {
      redapToken: "token-a",
      receiver: { kind: "archive", archive },
    }
  );

  assert.deepEqual(transition.urlsToCloseBeforeOpen, ["live-1"]);
  assert.equal(transition.receiverUrlToOpen, "rerun://archive");
});

test("new archive layers replace the same receiver without overlapping Store IDs", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      receiver: {
        kind: "archive",
        archive: { uri: "rerun://archive", revision: "revision-a" },
      },
    },
    {
      redapToken: "token-a",
      receiver: {
        kind: "archive",
        archive: { uri: "rerun://archive", revision: "revision-b" },
      },
    }
  );

  assert.deepEqual(transition.urlsToCloseBeforeOpen, ["rerun://archive"]);
  assert.equal(transition.receiverUrlToOpen, "rerun://archive");
});

test("session renewal changes credentials without churning the receiver", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      receiver: { kind: "live", url: "live-1", generation: 0 },
    },
    {
      redapToken: "token-b",
      receiver: { kind: "live", url: "live-1", generation: 0 },
    }
  );

  assert.equal(transition.credentialsChanged, true);
  assert.equal(transition.receiverUrlToOpen, undefined);
  assert.deepEqual(transition.urlsToCloseBeforeOpen, []);
});

test("opens live playback through Rerun's native streaming receiver", () => {
  const transition = planRerunSourceTransition(
    {},
    {
      redapToken: "token-a",
      receiver: {
        kind: "live",
        url: "https://console.example/live.rrd",
        generation: 0,
      },
    }
  );

  assert.equal(transition.receiverUrlToOpen, "https://console.example/live.rrd");
  assert.deepEqual(transition.urlsToCloseBeforeOpen, []);
});

test("only Redap archive playback schedules credential renewal", () => {
  assert.equal(
    requiresPlaybackCredentialRenewal({
      kind: "live",
      url: "https://console.example/live.rrd",
      generation: 0,
    }),
    false
  );
  assert.equal(
    requiresPlaybackCredentialRenewal({
      kind: "archive",
      archive: { uri: "rerun+https://console.example/dataset/entry", revision: "r1" },
    }),
    true
  );
  assert.equal(requiresPlaybackCredentialRenewal(undefined), false);
});

test("a reconnect replaces the native live receiver without overlapping it", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      receiver: { kind: "live", url: "live-1", generation: 0 },
    },
    {
      redapToken: "token-a",
      receiver: { kind: "live", url: "live-1", generation: 1 },
    }
  );

  assert.deepEqual(transition.urlsToCloseBeforeOpen, ["live-1"]);
  assert.equal(transition.receiverUrlToOpen, "live-1");
});
