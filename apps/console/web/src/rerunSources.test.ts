import assert from "node:assert/strict";
import test from "node:test";

import {
  planRerunSourceTransition,
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
    receiver: { kind: "live", url: "https://console.example/live.rrd" },
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
      receiver: { kind: "live", url: "https://console.example/live.rrd" },
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

test("switches live to history by closing the old recording receiver first", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      receiver: { kind: "live", url: "live-1" },
    },
    {
      redapToken: "token-a",
      receiver: { kind: "archive", archive },
    }
  );

  assert.deepEqual(transition.urlsToCloseBeforeOpen, ["live-1"]);
  assert.equal(transition.receiverUrlToOpen, "rerun://archive");
  assert.equal(transition.followReceiver, false);
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
      receiver: { kind: "live", url: "live-1" },
    },
    {
      redapToken: "token-b",
      receiver: { kind: "live", url: "live-1" },
    }
  );

  assert.equal(transition.credentialsChanged, true);
  assert.equal(transition.receiverUrlToOpen, undefined);
  assert.deepEqual(transition.urlsToCloseBeforeOpen, []);
});

test("live receiver opens in Rerun follow mode without source rotation", () => {
  const transition = planRerunSourceTransition(
    {},
    {
      redapToken: "token-a",
      receiver: { kind: "live", url: "https://console.example/live.rrd" },
    }
  );

  assert.equal(transition.receiverUrlToOpen, "https://console.example/live.rrd");
  assert.equal(transition.followReceiver, true);
  assert.deepEqual(transition.urlsToCloseBeforeOpen, []);
});
