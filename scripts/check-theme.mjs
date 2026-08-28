/**
 * Assertions over the theme modules, which are pure TypeScript with no DOM or
 * Tauri behind them. esbuild is already a dependency of Vite, so this runs
 * without adding a test runner to the project.
 *
 *   node scripts/check-theme.mjs
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
   export { PRESET_THEMES, FALLBACK_THEME_ID } from "${process.cwd()}/src/theme/presets";`,
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

console.log(`\n${checks} checks passed`);
