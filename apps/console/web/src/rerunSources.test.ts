import assert from "node:assert/strict";
import test from "node:test";

import { planRerunSourceTransition } from "./rerunSources.ts";

test("opens one lazy Redap archive receiver", () => {
  const transition = planRerunSourceTransition(
    {},
    {
      redapToken: "token-a",
      archive: { uri: "rerun://archive", revision: "revision-a" },
    }
  );

  assert.equal(transition.credentialsChanged, true);
  assert.equal(transition.archiveUrlToOpen, "rerun://archive");
  assert.equal(transition.archiveUrlToCloseBeforeOpen, undefined);
  assert.deepEqual(transition.urlsToCloseAfterOpen, []);
});

test("opens the governed producer Blueprint before recording sources", () => {
  const transition = planRerunSourceTransition(
    {},
    {
      redapToken: "token-a",
      blueprintUrl: "https://console.example/blueprints/1.rrd",
      archive: { uri: "rerun://archive", revision: "revision-a" },
    }
  );

  assert.equal(
    transition.blueprintUrlToOpen,
    "https://console.example/blueprints/1.rrd"
  );
  assert.equal(transition.archiveUrlToOpen, "rerun://archive");
});

test("replaces a producer Blueprint revision without retaining the previous source", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      blueprintUrl: "https://console.example/blueprints/1.rrd",
    },
    {
      redapToken: "token-a",
      blueprintUrl: "https://console.example/blueprints/2.rrd",
    }
  );

  assert.equal(
    transition.blueprintUrlToOpen,
    "https://console.example/blueprints/2.rrd"
  );
  assert.deepEqual(transition.urlsToCloseAfterOpen, [
    "https://console.example/blueprints/1.rrd",
  ]);
});

test("rollover attaches archive and successor live source before detaching old live", () => {
  const transition = planRerunSourceTransition(
    { redapToken: "token-a", liveUrl: "live-1" },
    {
      redapToken: "token-a",
      archive: { uri: "rerun://archive", revision: "revision-a" },
      liveUrl: "live-2",
    }
  );

  assert.equal(transition.archiveUrlToOpen, "rerun://archive");
  assert.equal(transition.liveUrlToOpen, "live-2");
  assert.deepEqual(transition.urlsToCloseAfterOpen, ["live-1"]);
});

test("new immutable layers refresh the same stable archive receiver", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      archive: { uri: "rerun://archive", revision: "revision-a" },
    },
    {
      redapToken: "token-a",
      archive: { uri: "rerun://archive", revision: "revision-b" },
    }
  );

  assert.equal(transition.archiveUrlToCloseBeforeOpen, "rerun://archive");
  assert.equal(transition.archiveUrlToOpen, "rerun://archive");
  assert.deepEqual(transition.urlsToCloseAfterOpen, []);
});

test("session renewal requires a new viewer credential context without churning URLs", () => {
  const transition = planRerunSourceTransition(
    {
      redapToken: "token-a",
      archive: { uri: "rerun://archive", revision: "revision-a" },
      liveUrl: "live-1",
    },
    {
      redapToken: "token-b",
      archive: { uri: "rerun://archive", revision: "revision-a" },
      liveUrl: "live-1",
    }
  );

  assert.equal(transition.credentialsChanged, true);
  assert.equal(transition.archiveUrlToOpen, undefined);
  assert.equal(transition.liveUrlToOpen, undefined);
  assert.deepEqual(transition.urlsToCloseAfterOpen, []);
});
