/** Build the durable browser-session shape used by PC Snapshot snapshots. */

const TAB_GROUP_ID_NONE = -1;

/** @param {unknown} value */
function finiteNumberOrNull(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** @param {string} url */
function isRestorableUrl(url) {
  return /^(https?|file):\/\//i.test(url);
}

/**
 * Runtime IDs are intentionally converted to snapshot-local keys: tab, group,
 * and window IDs are invalid after a browser restart.
 *
 * @param {object} api WebExtension API namespace.
 * @param {string} browserFamily Browser family/name from the background worker.
 * @param {string} profileInstanceId Extension-storage-backed profile identity.
 */
export async function captureBrowserSession(api, browserFamily, profileInstanceId) {
  const rawWindows = await api.windows.getAll({ populate: true });
  const windows = [];
  let supportsTabGroups = typeof api.tabGroups?.query === "function";

  for (const rawWindow of rawWindows) {
    // Private, popup, and devtools windows are never part of a durable restore.
    if (rawWindow.incognito || rawWindow.type !== "normal") continue;

    const windowOrdinal = windows.length;
    const rawTabs = rawWindow.tabs ?? await api.tabs.query({ windowId: rawWindow.id });
    const sortedTabs = [...rawTabs].sort((a, b) => a.index - b.index);
    const groupsByRuntimeId = new Map();

    if (supportsTabGroups) {
      try {
        const rawGroups = await api.tabGroups.query({ windowId: rawWindow.id });
        for (let index = 0; index < rawGroups.length; index += 1) {
          const group = rawGroups[index];
          groupsByRuntimeId.set(group.id, {
            key: `g:${windowOrdinal}:${index}`,
            title: group.title ?? "",
            color: group.color ?? "grey",
            collapsed: Boolean(group.collapsed),
            index: finiteNumberOrNull(group.index),
          });
        }
      } catch {
        // A partial API must not turn into a partial browser capture.
        supportsTabGroups = false;
        groupsByRuntimeId.clear();
      }
    }

    const tabs = sortedTabs.map((tab) => {
      const url = typeof tab.url === "string"
        ? tab.url
        : typeof tab.pendingUrl === "string"
          ? tab.pendingUrl
          : "";
      const group = groupsByRuntimeId.get(tab.groupId ?? TAB_GROUP_ID_NONE);
      return {
        url,
        title: typeof tab.title === "string" ? tab.title : "",
        index: tab.index,
        active: Boolean(tab.active),
        pinned: Boolean(tab.pinned),
        muted: Boolean(tab.mutedInfo?.muted ?? tab.muted),
        discarded: Boolean(tab.discarded),
        group_key: group?.key ?? null,
        restorable: isRestorableUrl(url),
      };
    });

    windows.push({
      ordinal: windowOrdinal,
      bounds: {
        left: finiteNumberOrNull(rawWindow.left),
        top: finiteNumberOrNull(rawWindow.top),
        width: finiteNumberOrNull(rawWindow.width),
        height: finiteNumberOrNull(rawWindow.height),
      },
      state: typeof rawWindow.state === "string" ? rawWindow.state : "normal",
      focused: Boolean(rawWindow.focused),
      tabs,
      groups: [...groupsByRuntimeId.values()],
    });
  }

  return {
    protocol_version: 1,
    browser: {
      family: browserFamily,
      profile_instance_id: profileInstanceId,
    },
    captured_at: new Date().toISOString(),
    capabilities: { tab_groups: supportsTabGroups },
    windows,
  };
}
