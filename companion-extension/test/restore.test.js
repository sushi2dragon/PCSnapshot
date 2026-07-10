import assert from "node:assert/strict";
import test from "node:test";
import { planTabReconciliation } from "../src/restore.js";

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
