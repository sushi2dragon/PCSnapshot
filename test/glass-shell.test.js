import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [appSource, cssSource, tauriConfigSource] = await Promise.all([
  readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
  readFile(new URL("../src/index.css", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
]);

function assertGlassShell({ app = appSource, css = cssSource, config = tauriConfigSource } = {}) {
  const tauriConfig = JSON.parse(config);
  const mainWindow = tauriConfig.app.windows.find((window) => window.label === "main");
  assert.equal(mainWindow?.transparent, true, "the main native window must stay transparent");
  assert.equal("backgroundColor" in mainWindow, false, "the native window must not paint an opaque background");

  assert.match(app, /className="app-loading[^\"]*"/, "the loading path must use the glass-safe root");
  assert.match(app, /className="app-surface"/, "the loaded path must use the glass-safe root");
  assert.doesNotMatch(
    app,
    /backgroundColor\s*:\s*["']var\(--bg-base\)["']/,
    "React must not repaint the full viewport with the opaque base color",
  );

  assert.match(css, /html,body,#root\s*\{\s*background\s*:\s*transparent\s*\}/);
  assert.match(css, /\.app-surface\s*\{[^}]*background\s*:\s*transparent[^}]*\}/s);
  assert.match(css, /\.app-loading\s*\{[^}]*background\s*:\s*transparent[^}]*\}/s);

  const activityRules = [...css.matchAll(/\.right-panel\s*\{([^}]*)\}/gs)];
  const activityRule = activityRules.at(-1)?.[1] ?? "";
  assert.match(activityRule, /background\s*:\s*linear-gradient\(/);
  assert.doesNotMatch(activityRule, /data:image|url\(/, "Activity must not add a texture over native Acrylic");
  assert.doesNotMatch(
    activityRule,
    /background-image\s*:\s*none/,
    "Activity must not cancel its Settings-inspired gradient",
  );

  const finalBackgroundAlpha = (selector) => {
    const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const rules = [...css.matchAll(new RegExp(`${escapedSelector}\\s*\\{([^}]*)\\}`, "gs"))];
    const finalRule = rules.at(-1)?.[1] ?? "";
    const background = finalRule.match(/background(?:-color)?\s*:\s*([^;]+)/)?.[1] ?? "";
    const alphas = [...background.matchAll(/rgba\([^)]*,\s*(0?\.\d+|1)\s*\)/g)].map((match) => Number(match[1]));
    assert.ok(alphas.length, `${selector} must have an rgba glass tint`);
    return Math.max(...alphas);
  };

  const centerAlpha = finalBackgroundAlpha(".center-panel");
  const sidebarAlpha = finalBackgroundAlpha(".sidebar");
  const activityAlpha = finalBackgroundAlpha(".right-panel");
  assert.ok(centerAlpha <= 0.3, "the center must leave the native backdrop clearly visible");
  assert.ok(sidebarAlpha <= 0.5, "the sidebar must remain translucent");
  assert.ok(activityAlpha <= 0.55, "Activity must remain translucent");
  assert.ok(sidebarAlpha - centerAlpha >= 0.12, "the sidebar must stay darker than the center");
  assert.ok(activityAlpha - centerAlpha >= 0.18, "Activity must stay darker than the center");
}

test("main and Activity surfaces leave the native Acrylic backdrop visible", () => {
  assertGlassShell();
});

test("Settings-inspired shell stays visibly translucent with a dark-blue hierarchy", () => {
  assertGlassShell();
});

test("gate rejects the opaque React root that caused the resize-only glass", () => {
  const sabotagedApp = appSource.replace(
    'className="app-surface"',
    'style={{ height: "100vh", backgroundColor: "var(--bg-base)" }}',
  );
  assert.throws(() => assertGlassShell({ app: sabotagedApp }), /glass-safe root|opaque base color/);
});
