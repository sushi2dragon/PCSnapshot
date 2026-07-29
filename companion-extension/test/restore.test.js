import assert from "node:assert/strict";
import test from "node:test";
import { planTabReconciliation, reconcileBrowserSession } from "../src/restore.js";

/** Minimal WebExtension tabs/windows stub that records what was asked of it. */
function fakeApi(liveWindows) {
  let nextId = 1000;
  return {
    windows: {
      getAll: async () => liveWindows,
      create: async () => ({ id: (nextId += 1), tabs: [{ id: (nextId += 1) }] }),
      update: async () => {},
      remove: async () => {},
    },
    tabs: {
      create: async () => ({ id: (nextId += 1) }),
      move: async () => {},
      update: async () => {},
      remove: async () => {},
    },
  };
}

test("plans duplicate URLs by occurrence and leaves only unmatched tabs extra", () => {
  const live = [{ id: 1, tabs: [
    { id: 10, index: 0, url: "https://same.example" },
    { id: 11, index: 1, url: "https://same.example" },
    { id: 12, index: 2, url: "https://extra.example" },
  ] }];
  const target = {
    windows: [{ tabs: [
      { index: 0, url: "https://same.example", restorable: true },
      { index: 1, url: "https://same.example", restorable: true },
      { index: 2, url: "https://missing.example", restorable: true },
      { index: 3, url: "chrome://settings", restorable: false },
    ] }],
  };

  const plan = planTabReconciliation(live, target);

  assert.deepEqual(plan.windows[0].tabs.map((tab) => tab.action), ["reuse", "reuse", "create", "skip"]);
  assert.deepEqual(plan.windows[0].tabs.slice(0, 2).map((tab) => tab.live.tab.id), [10, 11]);
  assert.deepEqual(plan.extras.map((tab) => tab.tab.id), [12]);
});

test("a clean restore reports no warnings for pages that were never restorable", async () => {
  // Every profile carries a new-tab page or a settings tab. Counting those as
  // skipped work made the desktop app surface a warning on every browser
  // restore, which is what taught the user to read a working restore as broken.
  const live = [{ id: 1, type: "normal", incognito: false, tabs: [
    { id: 10, index: 0, url: "https://keep.example" },
  ] }];
  const target = {
    capabilities: { tab_groups: false },
    windows: [{ bounds: {}, state: "normal", groups: [], tabs: [
      { index: 0, url: "https://keep.example", restorable: true },
      { index: 1, url: "chrome://newtab/", restorable: false },
      { index: 2, url: "https://new.example", restorable: true },
    ] }],
  };

  const report = await reconcileBrowserSession(fakeApi(live), target, false);

  assert.deepEqual(report.warnings, []);
  assert.equal(report.ignored, 1);
  assert.equal(report.skipped, 0);
  assert.equal(report.reused, 1);
  assert.equal(report.opened, 1);
});
