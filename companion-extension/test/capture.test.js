import assert from "node:assert/strict";
import test from "node:test";
import { captureBrowserSession } from "../src/capture.js";

test("captures normal windows, duplicate tabs, groups, and excludes private windows", async () => {
  const api = {
    windows: {
      getAll: async () => [
        {
          id: 41, type: "normal", left: 10, top: 20, width: 1000, height: 700,
          focused: true, state: "normal",
          tabs: [
            { index: 1, url: "https://example.com", title: "second", groupId: 99 },
            { index: 0, url: "https://example.com", title: "first", active: true, pinned: true, groupId: 99 },
            { index: 2, url: "chrome://settings", title: "settings", groupId: -1 },
          ],
        },
        { id: 42, type: "normal", incognito: true, tabs: [{ index: 0, url: "https://private.example" }] },
      ],
    },
    tabs: { query: async () => assert.fail("populated windows must not query tabs") },
    tabGroups: {
      query: async ({ windowId }) => {
        assert.equal(windowId, 41);
        return [{ id: 99, title: "Research", color: "blue", collapsed: true, index: 0 }];
      },
    },
  };

  const session = await captureBrowserSession(api, "chrome", "profile-a");

  assert.equal(session.windows.length, 1);
  assert.deepEqual(session.windows[0].tabs.map((tab) => tab.url), [
    "https://example.com", "https://example.com", "chrome://settings",
  ]);
  assert.equal(session.windows[0].tabs[0].group_key, "g:0:0");
  assert.equal(session.windows[0].tabs[0].pinned, true);
  assert.equal(session.windows[0].tabs[2].restorable, false);
  assert.deepEqual(session.windows[0].groups, [{
    key: "g:0:0", title: "Research", color: "blue", collapsed: true, index: 0,
  }]);
});

test("falls back to tabs.query and degrades when tab groups are unavailable", async () => {
  const api = {
    windows: { getAll: async () => [{ id: 7, type: "normal", width: Number.NaN }] },
    tabs: {
      query: async ({ windowId }) => {
        assert.equal(windowId, 7);
        return [{ index: 0, pendingUrl: "file:///C:/notes.txt", mutedInfo: { muted: true } }];
      },
    },
  };

  const session = await captureBrowserSession(api, "firefox", "profile-b");

  assert.equal(session.capabilities.tab_groups, false);
  assert.equal(session.windows[0].bounds.width, null);
  assert.equal(session.windows[0].tabs[0].url, "file:///C:/notes.txt");
  assert.equal(session.windows[0].tabs[0].restorable, true);
  assert.equal(session.windows[0].tabs[0].muted, true);
});
