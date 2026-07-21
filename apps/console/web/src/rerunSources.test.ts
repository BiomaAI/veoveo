import assert from "node:assert/strict";
import test from "node:test";

import { planRerunSourceTransition } from "./rerunSources.ts";

test("opens complete archive history in one receiver set", () => {
  const transition = planRerunSourceTransition(
    { archiveUrls: new Set() },
    { archiveUrls: ["archive-0", "archive-1"] }
  );

  assert.deepEqual(transition.archiveUrlsToOpen, ["archive-0", "archive-1"]);
  assert.equal(transition.liveUrlToOpen, undefined);
  assert.deepEqual(transition.urlsToClose, []);
});

test("rollover attaches frozen history and successor live source before detaching", () => {
  const transition = planRerunSourceTransition(
    { archiveUrls: new Set(["archive-0"]), liveUrl: "live-1" },
    {
      archiveUrls: ["archive-0", "archive-1"],
      liveUrl: "live-2",
    }
  );

  assert.deepEqual(transition.archiveUrlsToOpen, ["archive-1"]);
  assert.equal(transition.liveUrlToOpen, "live-2");
  assert.deepEqual(transition.urlsToClose, ["live-1"]);
  assert.deepEqual([...transition.next.archiveUrls], ["archive-0", "archive-1"]);
});

test("stable playback-session renewal does not churn receivers", () => {
  const transition = planRerunSourceTransition(
    { archiveUrls: new Set(["archive-0"]), liveUrl: "live-1" },
    { archiveUrls: ["archive-0"], liveUrl: "live-1" }
  );

  assert.deepEqual(transition.archiveUrlsToOpen, []);
  assert.equal(transition.liveUrlToOpen, undefined);
  assert.deepEqual(transition.urlsToClose, []);
});

test("an expired playback session replaces sources without remounting the viewer", () => {
  const transition = planRerunSourceTransition(
    { archiveUrls: new Set(["session-a/archive-0"]), liveUrl: "session-a/live-1" },
    {
      archiveUrls: ["session-b/archive-0"],
      liveUrl: "session-b/live-1",
    }
  );

  assert.deepEqual(transition.archiveUrlsToOpen, ["session-b/archive-0"]);
  assert.equal(transition.liveUrlToOpen, "session-b/live-1");
  assert.deepEqual(transition.urlsToClose, ["session-a/archive-0", "session-a/live-1"]);
});
