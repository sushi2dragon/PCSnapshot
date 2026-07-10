/**
 * Plan URL reconciliation without mutating a browser. Exact browser-returned
 * URLs are the identity: query strings and fragments are not normalized away.
 * This preserves repeated URLs as separate tab occurrences.
 */
export function planTabReconciliation(liveWindows, targetSession) {
  const reusableByUrl = new Map();
  const allLive = [];

  for (const liveWindow of liveWindows) {
    for (const tab of [...(liveWindow.tabs ?? [])].sort((a, b) => a.index - b.index)) {
      const entry = { tab, window_id: liveWindow.id };
      allLive.push(entry);
      if (typeof tab.url === "string" && tab.url) {
        const matches = reusableByUrl.get(tab.url) ?? [];
        matches.push(entry);
        reusableByUrl.set(tab.url, matches);
      }
    }
  }

  const used = new Set();
  const windows = targetSession.windows.map((targetWindow) => ({
    target: targetWindow,
    tabs: targetWindow.tabs.map((targetTab) => {
      if (!targetTab.restorable) return { target: targetTab, action: "skip" };
      const matches = reusableByUrl.get(targetTab.url) ?? [];
      const reuse = matches.find((entry) => !used.has(entry.tab.id));
      if (reuse) {
        used.add(reuse.tab.id);
        return { target: targetTab, action: "reuse", live: reuse };
      }
      return { target: targetTab, action: "create" };
    }),
  }));

  return {
    windows,
    extras: allLive.filter((entry) => !used.has(entry.tab.id)),
  };
}

function normalWindows(rawWindows) {
  return rawWindows.filter((window) => window.type === "normal" && !window.incognito);
}

function boundsFor(target) {
  return Object.fromEntries(
    Object.entries(target.bounds ?? {}).filter(([, value]) => typeof value === "number" && Number.isFinite(value)),
  );
}

async function updateWindow(api, windowId, target, warnings) {
  try {
    const bounds = boundsFor(target);
    if (Object.keys(bounds).length) await api.windows.update(windowId, bounds);
    if (target.state && target.state !== "normal") {
      await api.windows.update(windowId, { state: target.state });
    }
  } catch (error) {
    console.error("PC Snapshot could not restore browser window layout", { windowId, error });
    warnings.push(`Could not restore browser window layout: ${String(error)}`);
  }
}

async function updateTab(api, tabId, target, warnings) {
  try {
    await api.tabs.update(tabId, { pinned: Boolean(target.pinned), muted: Boolean(target.muted) });
  } catch (error) {
    console.error("PC Snapshot could not restore browser tab options", { tabId, url: target.url, error });
    warnings.push(`Could not restore tab options for ${target.url}: ${String(error)}`);
  }
}

/**
 * Reconcile the current extension profile to a captured BrowserSession.
 * Target windows and tabs are created before extras are removed, so a failed
 * creation can never turn a clean restore into an empty browser.
 */
export async function reconcileBrowserSession(api, targetSession, closeExtras) {
  const rawWindows = await api.windows.getAll({ populate: true });
  const liveWindows = normalWindows(rawWindows);
  const plan = planTabReconciliation(liveWindows, targetSession);
  console.debug("PC Snapshot browser restore plan built", {
    liveWindows: liveWindows.length,
    targetWindows: targetSession.windows.length,
    plannedWindows: plan.windows.length,
    extraTabs: plan.extras.length,
    closeExtras,
  });
  const warnings = [];
  const createdBlankTabIds = [];
  const result = { reused: 0, opened: 0, closed: 0, skipped: 0, warnings };

  for (let ordinal = 0; ordinal < plan.windows.length; ordinal += 1) {
    const plannedWindow = plan.windows[ordinal];
    let runtimeWindow = liveWindows[ordinal];
    console.debug("PC Snapshot reconciling browser window", {
      window: ordinal + 1,
      tabCount: plannedWindow.tabs.length,
      existing: Boolean(runtimeWindow),
    });
    if (!runtimeWindow) {
      try {
        runtimeWindow = await api.windows.create({ ...boundsFor(plannedWindow.target), state: "normal" });
        for (const tab of runtimeWindow.tabs ?? []) createdBlankTabIds.push(tab.id);
        console.debug("PC Snapshot browser window created", { window: ordinal + 1, windowId: runtimeWindow.id });
      } catch (error) {
        console.error("PC Snapshot could not create browser window", { window: ordinal + 1, error });
        warnings.push(`Could not create browser window ${ordinal + 1}: ${String(error)}`);
        result.skipped += plannedWindow.tabs.length;
        continue;
      }
    }

    const runtimeTabByTargetIndex = new Map();
    for (const plannedTab of plannedWindow.tabs) {
      const { target } = plannedTab;
      if (plannedTab.action === "skip") {
        result.skipped += 1;
        warnings.push(`Skipped non-restorable browser page: ${target.url || target.title}`);
        continue;
      }
      let runtimeTabId;
      try {
        if (plannedTab.action === "reuse") {
          runtimeTabId = plannedTab.live.tab.id;
          await api.tabs.move(runtimeTabId, { windowId: runtimeWindow.id, index: target.index });
          result.reused += 1;
          console.debug("PC Snapshot browser tab reused", { window: ordinal + 1, tabId: runtimeTabId, url: target.url });
        } else {
          const created = await api.tabs.create({
            windowId: runtimeWindow.id,
            index: target.index,
            url: target.url,
            active: false,
            pinned: Boolean(target.pinned),
          });
          runtimeTabId = created.id;
          result.opened += 1;
          console.debug("PC Snapshot browser tab created", { window: ordinal + 1, tabId: runtimeTabId, url: target.url });
        }
        await updateTab(api, runtimeTabId, target, warnings);
        runtimeTabByTargetIndex.set(target.index, runtimeTabId);
      } catch (error) {
        console.error("PC Snapshot could not restore browser tab", { window: ordinal + 1, url: target.url, error });
        result.skipped += 1;
        warnings.push(`Could not restore ${target.url}: ${String(error)}`);
      }
    }

    const targetTabIds = [...runtimeTabByTargetIndex.values()];
    if (targetTabIds.length && typeof api.tabs.ungroup === "function") {
      try {
        await api.tabs.ungroup(targetTabIds);
      } catch (error) {
        console.error("PC Snapshot could not clear prior browser tab groups", {
          window: ordinal + 1,
          tabIds: targetTabIds,
          error,
        });
      }
    }
    if (targetSession.capabilities?.tab_groups && typeof api.tabs.group === "function") {
      for (const group of plannedWindow.target.groups ?? []) {
        const ids = plannedWindow.target.tabs
          .filter((tab) => tab.group_key === group.key)
          .map((tab) => runtimeTabByTargetIndex.get(tab.index))
          .filter((id) => typeof id === "number");
        if (!ids.length) continue;
        try {
          const groupId = await api.tabs.group({ tabIds: ids, windowId: runtimeWindow.id });
          if (typeof api.tabGroups?.update === "function") {
            await api.tabGroups.update(groupId, {
              title: group.title,
              color: group.color,
              collapsed: Boolean(group.collapsed),
            });
          }
          console.debug("PC Snapshot browser tab group restored", {
            window: ordinal + 1,
            groupId,
            title: group.title,
            tabCount: ids.length,
          });
        } catch (error) {
          console.error("PC Snapshot could not restore browser tab group", {
            window: ordinal + 1,
            title: group.title,
            error,
          });
          warnings.push(`Could not restore tab group ${group.title || "(untitled)"}: ${String(error)}`);
        }
      }
    }

    const active = plannedWindow.target.tabs.find((tab) => tab.active);
    const activeId = active && runtimeTabByTargetIndex.get(active.index);
    if (typeof activeId === "number") {
      try {
        await api.tabs.update(activeId, { active: true });
      } catch (error) {
        console.error("PC Snapshot could not restore active browser tab", {
          window: ordinal + 1,
          tabId: activeId,
          error,
        });
      }
    }
    await updateWindow(api, runtimeWindow.id, plannedWindow.target, warnings);
  }

  if (closeExtras) {
    // Live windows beyond the snapshot's window count are entirely extra. Close
    // the whole window rather than its tabs: removing every tab does not reliably
    // close a window (Opera GX keeps the window shell / GX Corner alive, and can
    // even inject a fresh tab), so an extra window would otherwise survive empty.
    const extraWindows = plan.windows.length > 0 ? liveWindows.slice(plan.windows.length) : [];
    const extraWindowIds = new Set(extraWindows.map((win) => win.id));
    for (const win of extraWindows) {
      const closedTabs = plan.extras.filter((entry) => entry.window_id === win.id).length;
      try {
        await api.windows.remove(win.id);
        result.closed += closedTabs;
        console.debug("PC Snapshot browser window closed", { windowId: win.id, closedTabs });
      } catch (error) {
        console.error("PC Snapshot could not close extra browser window", { windowId: win.id, error });
        warnings.push(`Could not close an extra browser window: ${String(error)}`);
      }
    }

    // Remaining extras live in windows we're keeping; close those individually.
    const extraIds = [...new Set([
      ...plan.extras.filter((entry) => !extraWindowIds.has(entry.window_id)).map((entry) => entry.tab.id),
      ...createdBlankTabIds,
    ])];
    if (extraIds.length) {
      try {
        await api.tabs.remove(extraIds);
        result.closed += extraIds.length;
        console.debug("PC Snapshot browser tabs closed", { tabIds: extraIds, count: extraIds.length });
      } catch (error) {
        console.error("PC Snapshot could not close extra browser tabs", { tabIds: extraIds, error });
        warnings.push(`Could not close ${extraIds.length} extra browser tab(s): ${String(error)}`);
      }
    }
  }

  return result;
}
