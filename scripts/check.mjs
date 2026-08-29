/**
 * Assertions over the modules that are pure TypeScript with no DOM, Excalidraw
 * or Tauri behind them — the theme engine and the tab model. esbuild is already
 * a dependency of Vite, so this runs without adding a test runner to the
 * project.
 *
 *   node scripts/check.mjs
 */
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
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
   export * from "${process.cwd()}/src/lib/tabs";`,
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

console.log(`\n${checks} checks passed`);
