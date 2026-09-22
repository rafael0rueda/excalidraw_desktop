/**
 * Assertions over the modules that are pure TypeScript with no DOM, Excalidraw
 * or Tauri behind them — the theme engine and the tab model. esbuild is already
 * a dependency of Vite, so this runs without adding a test runner to the
 * project.
 *
 *   node scripts/check.mjs
 */
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { build } from "esbuild";

const out = mkdtempSync(join(tmpdir(), "excalidraw-theme-"));
const entry = join(out, "entry.ts");
writeFileSync(
  entry,
  `export * from "${process.cwd()}/src/theme/draft";
   export * from "${process.cwd()}/src/theme/types";
   export * from "${process.cwd()}/src/theme/color";
   export { cssVariables } from "${process.cwd()}/src/theme/variables";
   export { PRESET_THEMES, FALLBACK_THEME_ID } from "${process.cwd()}/src/theme/presets";
   export * from "${process.cwd()}/src/lib/tabs";
   export * from "${process.cwd()}/src/lib/documentState";
   export * from "${process.cwd()}/src/lib/shortcuts";`,
);
const bundle = join(out, "bundle.mjs");
await build({ entryPoints: [entry], bundle: true, format: "esm", outfile: bundle, logLevel: "warning" });
const t = await import(bundle);
rmSync(out, { recursive: true, force: true });

let checks = 0;
const check = (name, fn) => {
  fn();
  checks++;
  console.log(`  ok  ${name}`);
};

check("every preset is a valid theme", () => {
  for (const preset of t.PRESET_THEMES) {
    const parsed = t.parseTheme(JSON.parse(JSON.stringify(preset)));
    assert.ok("theme" in parsed, `${preset.id}: ${parsed.error}`);
  }
  assert.ok(t.PRESET_THEMES.some((p) => p.id === t.FALLBACK_THEME_ID));
  const ids = t.PRESET_THEMES.map((p) => p.id);
  assert.equal(new Set(ids).size, ids.length, "preset ids must be unique");
});

check("preset ids are ids the backend will accept", () => {
  // Mirrors `theme_file` in src-tauri/src/settings.rs, which is what a user
  // theme overriding a preset has to satisfy.
  for (const { id } of t.PRESET_THEMES) assert.match(id, /^[a-z0-9-]{1,64}$/);
});

check("parseTheme reports what is missing rather than throwing", () => {
  assert.deepEqual(t.parseTheme(null), { error: "not a JSON object" });
  assert.deepEqual(t.parseTheme({}), { error: 'missing "id"' });
  assert.deepEqual(t.parseTheme({ id: "x" }), { error: 'x: missing "name"' });
  assert.deepEqual(t.parseTheme({ id: "x", name: "X" }), { error: 'x: missing "colors"' });
  const partial = { id: "x", name: "X", colors: { canvas: "#fff" } };
  assert.deepEqual(t.parseTheme(partial), { error: 'x: missing "colors.surface"' });
});

check("cssVariables emits well-formed declarations for every preset", () => {
  for (const preset of t.PRESET_THEMES) {
    const vars = t.cssVariables(preset);
    assert.ok(Object.keys(vars).length > 50, `${preset.id}: too few variables`);
    for (const [key, value] of Object.entries(vars)) {
      assert.match(key, /^--[a-z0-9-]+$/, `${preset.id}: bad property ${key}`);
      assert.match(value, /^(#[0-9a-f]{6}|rgba\(|transparent$)/i, `${preset.id}: bad value ${key}: ${value}`);
    }
  }
});

check("slugify produces backend-safe ids", () => {
  assert.equal(t.slugify("Kanagawa Wave"), "kanagawa-wave");
  assert.equal(t.slugify("  My Théme!! "), "my-th-me");
  assert.equal(t.slugify("---"), "");
  assert.match(t.slugify("x".repeat(200)), /^x{64}$/);
});

check("uniqueId walks past collisions", () => {
  assert.equal(t.uniqueId("nord", new Set()), "nord");
  assert.equal(t.uniqueId("nord", new Set(["nord"])), "nord-2");
  assert.equal(t.uniqueId("nord", new Set(["nord", "nord-2"])), "nord-3");
  assert.equal(t.uniqueId("", new Set()), "custom");
});

check("paintable repairs half-typed colours from the fallback", () => {
  const base = t.PRESET_THEMES[0];
  const typing = { ...base, colors: { ...base.colors, accent: "#12", fill: "transparent" } };
  const safe = t.paintable(typing, base);
  assert.equal(safe.colors.accent, base.colors.accent);
  assert.equal(safe.colors.fill, "transparent", "the transparent keyword must survive");
  assert.deepEqual(t.invalidKeys(typing), ["accent"]);
  assert.deepEqual(t.invalidKeys(base), []);
});

check("serializeTheme writes schema order and normalises scrambled files", () => {
  const preset = t.PRESET_THEMES[0];
  const text = t.serializeTheme(preset);
  assert.ok(text.endsWith("}\n"), "files should end with a newline");
  assert.deepEqual(Object.keys(JSON.parse(text)), ["id", "name", "dark", "colors"]);
  assert.deepEqual(Object.keys(JSON.parse(text).colors), t.THEME_COLOR_KEYS);

  // A file written by hand in any order comes back in schema order, because
  // parseTheme rebuilds `colors` by walking THEME_COLOR_KEYS.
  const scrambled = JSON.parse(text);
  scrambled.colors = Object.fromEntries(Object.entries(scrambled.colors).reverse());
  const parsed = t.parseTheme(scrambled);
  assert.ok("theme" in parsed);
  assert.equal(t.serializeTheme(parsed.theme), text);
});

check("a serialized theme parses back to the same theme", () => {
  for (const preset of t.PRESET_THEMES) {
    const parsed = t.parseTheme(JSON.parse(t.serializeTheme(preset)));
    assert.ok("theme" in parsed);
    assert.deepEqual(parsed.theme, preset);
  }
});

check("a tab is titled by its file, and an unsaved one is Untitled", () => {
  assert.equal(t.tabTitle({ id: "a", path: "/home/rafa/notes/plan.excalidraw", dirty: false }), "plan.excalidraw");
  assert.equal(t.tabTitle({ id: "a", path: null, dirty: true }), "Untitled");
  assert.equal(t.basename("plan.excalidraw"), "plan.excalidraw");
});

check("tab ids are ids the backend will accept as file names", () => {
  // Mirrors `safe_id` in src-tauri/src/store.rs, which is what a tab id has to
  // satisfy before it names a snapshot.
  for (let i = 0; i < 5; i++) assert.match(t.newTabId(), /^[a-z0-9-]{1,64}$/);
  assert.equal(new Set([t.newTabId(), t.newTabId(), t.newTabId()]).size, 3);
});

check("closing a tab hands over to the one that takes its place", () => {
  const tabs = ["a", "b", "c"].map((id) => ({ id, path: null, dirty: false }));
  assert.equal(t.successorId(tabs, "a"), "b");
  assert.equal(t.successorId(tabs, "b"), "c", "the tab sliding into the gap");
  assert.equal(t.successorId(tabs, "c"), "b", "nothing follows the last one");
  assert.equal(t.successorId([tabs[0]], "a"), null, "the last tab leaves nothing");
});

check("stepping through tabs wraps at both ends", () => {
  const tabs = ["a", "b", "c"].map((id) => ({ id, path: null, dirty: false }));
  assert.equal(t.relativeId(tabs, "a", 1), "b");
  assert.equal(t.relativeId(tabs, "c", 1), "a");
  assert.equal(t.relativeId(tabs, "a", -1), "c");
  assert.equal(t.relativeId([], "a", 1), null);
});

check("a file already open is found rather than opened twice", () => {
  const tabs = [
    { id: "a", path: "/tmp/one.excalidraw", dirty: false },
    { id: "b", path: null, dirty: false },
  ];
  assert.equal(t.findByPath(tabs, "/tmp/one.excalidraw").id, "a");
  assert.equal(t.findByPath(tabs, "/tmp/two.excalidraw"), undefined);
});

check("shortcuts follow the layout for letters and the position for digits", () => {
  assert.equal(t.shortcutKey("t", "KeyT"), "t");
  assert.equal(t.shortcutKey("S", "KeyS"), "s", "Shift does not change which shortcut it is");
  assert.equal(t.shortcutKey("z", "KeyW"), "z", "AZERTY: Ctrl+Z must not become Ctrl+W");
  assert.equal(t.shortcutKey("я", "KeyZ"), "z", "a Cyrillic layout falls back to the physical key");
  assert.equal(t.shortcutKey("&", "Digit1"), "1", "AZERTY's number row");
  assert.equal(t.shortcutKey("1", "Numpad1"), "1");
  assert.equal(t.shortcutKey("б", "Comma"), ",");
  assert.equal(t.shortcutKey("PageDown", "PageDown"), "PageDown");
});

check("the CSP lets the page fetch its own assets", () => {
  // Excalidraw inlines fonts into an SVG export by fetching the woff2 itself
  // (`fetchFont`, main chunk) — without 'self' the fetch is blocked, the
  // export still succeeds, and the text falls back to another font everywhere
  // but here. On-screen and PNG rendering go through font-src, so this hides.
  const config = JSON.parse(
    readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
  );
  const directives = Object.fromEntries(
    config.app.security.csp
      .split(";")
      .map((part) => part.trim().split(/\s+/))
      .filter(([name]) => name)
      .map(([name, ...values]) => [name, values]),
  );
  assert.ok(directives["connect-src"]?.includes("'self'"), "connect-src must allow 'self'");
  assert.ok(directives["connect-src"]?.includes("ipc:"), "connect-src must still allow Tauri's IPC");
  assert.ok(directives["object-src"]?.includes("'none'"), "object-src must stay closed");
});

// --- documentState: the decisions behind the autosave and startup paths.
// Each of these has been a bug at least once; none was reachable from here
// until the logic moved out of the `useDocument` hook.

const tab = (id, path = null, dirty = false) => ({ id, path, dirty });
const content = (scene, rev, savedVersion = 0) => ({ scene, view: null, savedVersion, rev });

check("a snapshot carries only the scenes the session file has not got", () => {
  const tabs = [tab("a", "/tmp/a.excalidraw"), tab("b"), tab("c")];
  const store = new Map([
    ["a", content("scene-a", 7)],
    ["b", content("scene-b", 2)],
    ["c", content("scene-c", 1)],
  ]);
  // `a` is on disk at the revision it still holds; `b` has moved since.
  const written = new Map([["a", 7], ["b", 1]]);

  const plan = t.snapshotPlan(tabs, "b", (id) => store.get(id), written);

  assert.deepEqual(plan.payload.map((p) => p.id), ["a", "b", "c"], "every tab is listed, or it is pruned");
  assert.equal(plan.payload[0].scene, undefined, "an untouched tab sends no scene");
  assert.equal(plan.payload[1].scene, "scene-b");
  assert.equal(plan.payload[2].scene, "scene-c", "a tab the file has never seen always sends one");
  assert.deepEqual([...plan.sent], [["b", 2], ["c", 1]]);
  assert.equal(plan.payload[0].path, "/tmp/a.excalidraw");
});

check("a snapshot with nothing new in it is not written", () => {
  const tabs = [tab("a"), tab("b")];
  const store = new Map([["a", content("x", 1)], ["b", content("y", 1)]]);
  const written = new Map([["a", 1], ["b", 1]]);
  const plan = (active) => t.snapshotPlan(tabs, active, (id) => store.get(id), written);

  const settled = plan("a");
  assert.equal(settled.sent.size, 0);
  assert.equal(t.worthWriting(settled, settled.meta), false, "pointer movement must not rewrite the file");
  assert.equal(t.worthWriting(plan("b"), settled.meta), true, "but switching tabs must");

  const renamed = t.snapshotPlan([tab("a", "/tmp/saved.excalidraw"), tab("b")], "a", (id) => store.get(id), written);
  assert.equal(t.worthWriting(renamed, settled.meta), true, "and so must a Save As");
  const marked = t.snapshotPlan([tab("a", null, true), tab("b")], "a", (id) => store.get(id), written);
  assert.equal(t.worthWriting(marked, settled.meta), true, "and an unsaved mark appearing");
});

check("what reached disk is recorded, and closed tabs are forgotten", () => {
  const tabs = [tab("a"), tab("b")];
  const store = new Map([["a", content("x", 4)], ["b", content("y", 9)]]);
  const plan = t.snapshotPlan(tabs, "a", (id) => store.get(id), new Map());

  const after = t.nextWritten(new Map([["gone", 3]]), plan, tabs);
  assert.deepEqual([...after].sort(), [["a", 4], ["b", 9]], "a tab that has closed leaves no revision behind");

  // The map handed in is not the map handed back: the caller keeps the old one
  // if the write it was planned for never lands.
  const before = new Map([["a", 1]]);
  t.nextWritten(before, plan, tabs);
  assert.deepEqual([...before], [["a", 1]]);
});

check("a revision moves only when the drawing does", () => {
  assert.equal(t.nextRev(undefined, "scene"), 1, "a tab nothing has stored yet starts at 1");
  assert.equal(t.nextRev(content("scene", 3), "scene"), 3, "the same text keeps its revision");
  assert.equal(t.nextRev(content("scene", 3), "other"), 4);
});

check("a capture mark moves with anything worth serialising for", () => {
  const mark = t.captureMark("a", 12, 3, "#1f1f28", 0);
  assert.equal(mark, t.captureMark("a", 12, 3, "#1f1f28", 0));
  assert.notEqual(mark, t.captureMark("b", 12, 3, "#1f1f28", 0), "another tab");
  assert.notEqual(mark, t.captureMark("a", 13, 3, "#1f1f28", 0), "an edit");
  assert.notEqual(mark, t.captureMark("a", 12, 2, "#1f1f28", 0), "a deletion");
  assert.notEqual(mark, t.captureMark("a", 12, 3, "#ffffff", 0), "the canvas colour");
  assert.notEqual(mark, t.captureMark("a", 12, 3, "#1f1f28", 1), "an image added");
});

check("only an unclean exit with unsaved work is worth a recovery prompt", () => {
  const scene = (id, path, dirty) => ({ id, path, dirty, scene: "{}" });
  const session = (tabs, clean_exit) => ({ tabs, active: null, clean_exit });

  assert.equal(t.recoverySubject(session([scene("a", null, true)], true)), null, "an orderly exit was already answered for");
  assert.equal(t.recoverySubject(session([scene("a", "/tmp/a.excalidraw", false)], false)), null, "nothing unsaved");
  assert.equal(
    t.recoverySubject(session([scene("a", "/home/rafa/plan.excalidraw", true)], false)),
    "plan.excalidraw was",
  );
  assert.equal(t.recoverySubject(session([scene("a", null, true)], false)), "An unsaved drawing was");
  assert.equal(
    t.recoverySubject(session([scene("a", null, true), scene("b", "/tmp/b.excalidraw", true), scene("c", null, false)], false)),
    "2 drawings were",
  );
});

check("a recovered drawing stays unsaved until it is saved", () => {
  const session = {
    tabs: [
      { id: "a", path: "/tmp/a.excalidraw", dirty: true, scene: "scene-a" },
      { id: "b", path: null, dirty: false, scene: "scene-b" },
    ],
    active: "b",
    clean_exit: false,
  };

  const restored = t.recoveredSession(session);

  assert.deepEqual(restored.tabs, [
    { id: "a", path: "/tmp/a.excalidraw", dirty: true },
    { id: "b", path: null, dirty: false },
  ]);
  assert.equal(restored.active, "b");
  assert.equal(restored.contents.get("a").scene, "scene-a");
  assert.equal(restored.contents.get("a").savedVersion, t.NEVER_SAVED, "it matches nothing on disk");
  assert.equal(restored.contents.get("b").savedVersion, t.UNPARSED, "worked out when it is first shown");
  assert.equal(restored.contents.get("a").view, null, "the snapshot's own appState carries the viewport");
  // The session file already holds these scenes, so the next snapshot has
  // nothing to send — sending them again would rewrite every tab at startup.
  assert.deepEqual([...restored.written], [["a", 1], ["b", 1]]);
  const plan = t.snapshotPlan(restored.tabs, restored.active, (id) => restored.contents.get(id), restored.written);
  assert.equal(plan.sent.size, 0);
});

check("the tab that comes back on screen is the one that was on it", () => {
  const tabs = [tab("a"), tab("b")];
  assert.equal(t.activeFrom(tabs, "b"), "b");
  assert.equal(t.activeFrom(tabs, "gone"), "a", "a tab that could not be reopened falls back to the first");
  assert.equal(t.activeFrom(tabs, null), "a");
  assert.equal(t.activeFrom([], "a"), null, "nothing opened at all");

  const session = { tabs: [{ id: "a", path: null, dirty: true, scene: "{}" }], active: "gone", clean_exit: false };
  assert.equal(t.recoveredSession(session).active, "a");
});

check("a tab reopened from its own file is clean and unparsed", () => {
  assert.deepEqual(t.reopenedContent("scene"), {
    scene: "scene",
    view: null,
    savedVersion: t.UNPARSED,
    rev: 1,
  });
});

console.log(`\n${checks} checks passed`);
