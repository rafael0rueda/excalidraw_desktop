/**
 * Assertions over the modules that are pure TypeScript with no DOM, Excalidraw
 * or Tauri behind them — the theme engine and the tab model. esbuild is already
 * a dependency of Vite, so this runs without adding a test runner to the
 * project.
 *
 *   node scripts/check.mjs
 */
import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { build } from "esbuild";
import * as erLibrary from "./build-er-library.mjs";
import * as kanagawaLibrary from "./build-kanagawa-erd-library.mjs";
import { versions } from "./version.mjs";

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
   export * from "${process.cwd()}/src/lib/selection";
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
  for (const { id } of t.PRESET_THEMES) assert.match(id, t.ID_PATTERN);
  // The Rust side's copy of the rule, which the renderer's must not outgrow.
  const store = readFileSync("src-tauri/src/store.rs", "utf8");
  assert.ok(store.includes(`id.len() <= ${t.MAX_ID_LENGTH}`), "safe_id's length limit");
  assert.ok(store.includes("c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'"), "safe_id's characters");
});

check("parseTheme reports what is missing rather than throwing", () => {
  assert.deepEqual(t.parseTheme(null), { error: "not a JSON object" });
  assert.deepEqual(t.parseTheme({}), { error: 'missing "id"' });
  assert.deepEqual(t.parseTheme({ id: "x" }), { error: 'x: missing "name"' });
  assert.deepEqual(t.parseTheme({ id: "x", name: "X" }), { error: 'x: missing "colors"' });
  const partial = { id: "x", name: "X", colors: { canvas: "#fff" } };
  assert.deepEqual(t.parseTheme(partial), { error: 'x: missing "colors.surface"' });
});

check("a theme colour is #rgb or #rrggbb, and transparent only as the fill", () => {
  for (const good of ["#fff", "#1F1F28", "#1f1f28"]) assert.equal(t.isColorValue("surface", good), true, good);
  // `1f1f28` was accepted once and written into a CSS variable as it stood;
  // `#1f1f28ff` is CSS, but the backend and the editor disagreed about it.
  for (const bad of ["1f1f28", "#1f1f28ff", "#12", "red", "", "transparent"]) {
    assert.equal(t.isColorValue("surface", bad), false, bad);
  }
  assert.equal(t.isColorValue("fill", "transparent"), true);

  const theme = JSON.parse(JSON.stringify(t.PRESET_THEMES[0]));
  const withColor = (key, value) => t.parseTheme({ ...theme, colors: { ...theme.colors, [key]: value } });
  assert.match(withColor("surface", "2A2A37").error, /"colors\.surface" is not a colour/);
  assert.match(withColor("accent", "transparent").error, /"colors\.accent" is not a colour/);
  assert.match(withColor("fill", "red").error, /or "transparent"/);

  // The id becomes a file name, so it is held to what the backend accepts.
  assert.match(t.parseTheme({ ...theme, id: "My Theme" }).error, /"id" may only use/);
  assert.match(t.parseTheme({ ...theme, id: "../escape" }).error, /"id" may only use/);
  assert.match(t.parseTheme({ ...theme, name: "   " }).error, /missing "name"/);
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
  assert.equal(t.slugify("x".repeat(200)), "x".repeat(t.MAX_ID_LENGTH));
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

check("Save writes in place only to a name the backend will write", () => {
  assert.equal(t.savesInPlace("/home/rafa/plan.excalidraw"), true);
  assert.equal(t.savesInPlace("/home/rafa/PLAN.EXCALIDRAW"), true);
  // Opened by content, from the file manager or the command line.
  assert.equal(t.savesInPlace("/home/rafa/plan"), false);
  assert.equal(t.savesInPlace("/home/rafa/drawing.json"), false);
  assert.equal(t.savesInPlace("/home/rafa/notes.excalidraw/plan"), false, "the directory's name is not the file's");
});

check("tab ids are ids the backend will accept as file names", () => {
  // Mirrors `safe_id` in src-tauri/src/store.rs, which is what a tab id has to
  // satisfy before it names a snapshot.
  for (let i = 0; i < 5; i++) assert.match(t.newTabId(), t.ID_PATTERN);
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

check("exporting the selection takes labels and frame contents with it", () => {
  const all = [
    { id: "box", frameId: null },
    { id: "box-label", frameId: null, containerId: "box" },
    { id: "frame", frameId: null },
    { id: "in-frame", frameId: "frame" },
    // Bound to something a frame pulls in: neither selected nor framed itself.
    { id: "in-frame-label", frameId: null, containerId: "in-frame" },
    { id: "elsewhere", frameId: null },
    { id: "elsewhere-label", frameId: null, containerId: "elsewhere" },
  ];
  const picked = (ids) => [...t.expandSelection(all, Object.fromEntries(ids.map((id) => [id, true])))].sort();

  assert.deepEqual(picked(["box"]), ["box", "box-label"], "a container brings its label");
  assert.deepEqual(picked(["frame"]), ["frame", "in-frame", "in-frame-label"], "a frame brings what is in it");
  assert.deepEqual(picked([]), []);
  // Excalidraw keeps deselected ids in the map as `false`.
  assert.deepEqual([...t.expandSelection(all, { box: false, elsewhere: true })].sort(), ["elsewhere", "elsewhere-label"]);
});

check("every command the renderer invokes exists, is registered, and takes the arguments it is sent", () => {
  // Nothing else would notice a rename: the app cannot be driven headlessly,
  // so a mismatch first shows as "command not found" in a window (review
  // 2026-11-04, ST-5). Tauri maps camelCase argument names to snake_case.
  const api = readFileSync("src/lib/api.ts", "utf8");
  const snake = (name) => name.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
  const sent = new Map();
  for (const m of api.matchAll(/invoke<[^>]*>\("([a-z_]+)"(?:,\s*(\{[^}]*\}|\w+))?/g)) {
    const arg = m[2];
    // A raw body (`invoke(cmd, bytes)`) reaches the command as its request.
    const args = !arg ? [] : arg.startsWith("{") ? arg.slice(1, -1).split(",").map((a) => a.trim().split(":")[0].trim()).filter(Boolean).map(snake).sort() : null;
    sent.set(m[1], args);
  }
  const rust = new Map();
  for (const file of readdirSync("src-tauri/src")) {
    const src = readFileSync(join("src-tauri/src", file), "utf8");
    for (const m of src.matchAll(/#\[tauri::command[^\]]*\]\s*(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?fn\s+([a-z_]+)(?:<[^>]*>)?\(([^)]*)\)/g)) {
      const params = m[2].split(/,(?![^<]*>)/).map((p) => p.trim()).filter(Boolean);
      const injected = /State<|AppHandle|Window\b|WebviewWindow|Request</;
      rust.set(m[1], {
        args: params.filter((p) => !injected.test(p)).map((p) => p.split(":")[0].replace(/^mut\s+/, "").trim()).sort(),
        raw: params.some((p) => /Request</.test(p)),
      });
    }
  }
  const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
  const registered = new Set(
    lib.split("generate_handler![")[1].split("]")[0].split(",").map((n) => n.trim().split("::").pop()).filter(Boolean),
  );
  assert.ok(sent.size > 20, "the invoke pattern still finds the calls");
  for (const [name, args] of sent) {
    const command = rust.get(name);
    assert.ok(command, `${name}: no #[tauri::command] by that name`);
    assert.ok(registered.has(name), `${name}: not in generate_handler!`);
    if (args === null) assert.ok(command.raw, `${name}: sent a raw body, but takes no Request`);
    else assert.deepEqual(args, command.args, `${name}: arguments`);
  }
  for (const name of registered) assert.ok(sent.has(name), `${name}: registered but never invoked`);

  // The events and the header, which are spelled once on each side.
  const constant = (src, name) => new RegExp(`${name}(?::\\s*&str)?\\s*=\\s*"([^"]+)"`).exec(src)?.[1];
  const settings = readFileSync("src-tauri/src/settings.rs", "utf8");
  const files = readFileSync("src-tauri/src/files.rs", "utf8");
  assert.equal(constant(api, "OPEN_FILES_EVENT"), constant(lib, "OPEN_FILES_EVENT"));
  assert.equal(constant(api, "COLOR_SCHEME_EVENT"), constant(settings, "COLOR_SCHEME_EVENT"));
  assert.ok(api.includes(`"${constant(files, "PATH_HEADER")}"`), "the x-path header");
});

check("the README's shortcut table is the table the app binds", () => {
  const readme = readFileSync("README.md", "utf8");
  const section = readme.split("## Keyboard shortcuts")[1].split("\n## ")[0];
  const rows = section.split("\n").filter((line) => /^\| `/.test(line));
  const documented = new Set(rows.flatMap((row) => [...row.split("|")[1].matchAll(/`([^`]+)`/g)].map((m) => m[1])));
  const bound = new Set([...t.SHORTCUTS.map(t.shortcutLabel), "Ctrl+1", "Ctrl+9"]);
  assert.deepEqual([...documented].sort(), [...bound].sort());
});

check("each shortcut is bound once, and each menu action has exactly one accelerator", () => {
  const keys = t.SHORTCUTS.map((s) => `${s.key}/${s.shift}`);
  assert.equal(new Set(keys).size, keys.length, "no key bound twice");
  for (const action of new Set(t.SHORTCUTS.map((s) => s.action))) {
    assert.equal(t.SHORTCUTS.filter((s) => s.action === action && s.menu).length, 1, action);
  }
  assert.equal(t.accelerator("saveAs"), "CmdOrCtrl+Shift+S");
  assert.equal(t.accelerator("nextTab"), "CmdOrCtrl+PageDown");
  assert.equal(t.shortcutAction("s", true), "saveAs");
  assert.equal(t.shortcutAction("PageUp", true), "previousTab", "Shift does not change PageUp");
  assert.equal(t.shortcutAction("Tab", true), "previousTab");
  assert.equal(t.shortcutAction("q", false), "quit", "Ctrl+Q is in the listener too");
  assert.equal(t.shortcutAction("p", true), null, "Ctrl+Shift+P is Excalidraw's");
});

check("a default colour nobody chose is free for a theme to replace", () => {
  assert.ok(t.isUnclaimed("#1E1E1E", t.FACTORY.stroke), "Excalidraw's own, in any case");
  assert.ok(t.isUnclaimed("#DCD7BA", t.FACTORY.stroke, "#DCD7BA"), "still the outgoing theme's");
  assert.ok(!t.isUnclaimed("#ff0000", t.FACTORY.stroke, "#DCD7BA"), "picked by hand");
  assert.ok(t.isUnclaimed("transparent", t.FACTORY.fill));
  assert.ok(!t.isUnclaimed("#ff0000", t.FACTORY.stroke, null));
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

check("every place the version is written agrees", () => {
  const found = versions();
  const distinct = [...new Set(found.map(([, version]) => version))];
  assert.equal(
    distinct.length,
    1,
    `run \`npm run bump -- <version>\`:\n${found.map(([file, version]) => `  ${version}  ${file}`).join("\n")}`,
  );
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
  // A drawing is untrusted input. These are what keep a hostile one from
  // running script or pulling a remote page into a frame: frames fall back to
  // default-src, and `lib.rs` would hand a refused frame's URL to the browser.
  assert.deepEqual(directives["default-src"], ["'self'"], "default-src must stay 'self' alone");
  for (const name of ["frame-src", "child-src"]) {
    assert.equal(directives[name], undefined, `${name} must not widen what default-src allows`);
  }
  for (const loose of ["'unsafe-inline'", "'unsafe-eval'", "data:", "blob:", "*"]) {
    assert.ok(!directives["script-src"]?.includes(loose), `script-src must not allow ${loose}`);
  }
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

check("an image a drawing does not actually contain is dropped on the way in", () => {
  const files = {
    a: { id: "a", dataURL: "data:image/png;base64,AAAA" },
    b: { id: "b", dataURL: "https://example.com/beacon.png" },
    c: { id: "c", dataURL: "DATA:IMAGE/svg+xml;base64,AAAA" },
    d: { id: "d", dataURL: "data:text/html,<script>1</script>" },
    e: { id: "e" },
  };
  assert.deepEqual(Object.keys(t.embeddedFiles(files)), ["a", "c"]);
  assert.deepEqual(t.embeddedFiles(null), {});
});

check("the canvas is a tab's only once that tab's scene has been committed", () => {
  // Tab `a` is on screen and the user switches to `b`.
  let canvas = t.canvasChanged(t.canvasHanded("a"));
  assert.equal(t.canvasHolds(canvas, "a"), true);
  assert.equal(t.canvasHolds(canvas, "b"), false, "the active id moves before the canvas does");

  canvas = t.CANVAS_IN_FLIGHT;
  // Excalidraw reports a change of its own while `b` is still being parsed.
  canvas = t.canvasChanged(canvas);
  assert.equal(t.canvasHolds(canvas, "b"), false, "a change mid-switch is not the new scene landing");
  assert.equal(t.canvasHolds(canvas, "a"), false, "and the tab being left has been captured already");

  canvas = t.canvasHanded("b");
  assert.equal(t.canvasHolds(canvas, "b"), false, "handed over is not yet on screen");
  canvas = t.canvasChanged(canvas);
  assert.equal(t.canvasHolds(canvas, "b"), true);
  assert.equal(t.canvasChanged(canvas), canvas, "later changes leave it alone");
});

check("a canvas reset is told apart from a theme that is simply white", () => {
  assert.equal(t.canvasWasReset("#ffffff", "#1f1f28"), true, "Reset the canvas on a dark theme");
  assert.equal(t.canvasWasReset("#FFFFFF", "#1f1f28"), true);
  assert.equal(t.canvasWasReset("#1f1f28", "#1f1f28"), false);
  assert.equal(t.canvasWasReset("#ffffff", "#ffffff"), false, "nothing to put back");
  assert.equal(t.canvasWasReset("#fdf6e3", "#1f1f28"), false, "a colour picked by hand is left alone");
});

check("a save keeps what was drawn while it was being written", () => {
  // Nothing moved during the write: the tab is clean at the version that went out.
  const quiet = t.afterWrite(content("sent", 4, 10), "sent", 12);
  assert.equal(quiet.dirty, false);
  assert.deepEqual(quiet.content, content("sent", 4, 12));
  // A tab that was never on the canvas works its version out when it is shown.
  assert.equal(t.afterWrite(content("sent", 4, 10), "sent", null).content.savedVersion, t.UNPARSED);

  // A stroke was drawn and the tab left before the write returned.
  const drawn = t.afterWrite(content("sent plus a stroke", 5, 10), "sent", 12);
  assert.equal(drawn.dirty, true, "the file does not have the stroke");
  assert.equal(drawn.content.scene, "sent plus a stroke", "and the store must not lose it");
  assert.equal(drawn.content.rev, 5, "nor rewind, or autosave skips it");
  assert.equal(drawn.content.savedVersion, 12);
  assert.equal(
    t.afterWrite(content("sent plus a stroke", 5, 10), "sent", null).content.savedVersion,
    t.NEVER_SAVED,
    "with no version to compare against, it stays unsaved until it is saved",
  );
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

check("the bundled libraries are exactly what their generators write", () => {
  // Excalidraw tells library items apart by element id and versionNonce. A
  // generator that drifts from its committed file hands everyone who already
  // has the shapes a second copy of each, so the bytes are held still here.
  const committed = readFileSync(erLibrary.OUT_PATH, "utf8");
  assert.equal(erLibrary.buildLibrary(committed), committed, "run: node scripts/build-er-library.mjs");
  for (const [name, text] of Object.entries(kanagawaLibrary.buildLibraries())) {
    assert.equal(
      text,
      readFileSync(join(kanagawaLibrary.OUT_DIR, name), "utf8"),
      `${name}: run node scripts/build-kanagawa-erd-library.mjs`,
    );
  }
});

check("every bundled library is one Excalidraw 0.18.1 can insert", () => {
  // Arrowheads and font ids as 0.18.1 defines them; restore quietly drops or
  // swaps anything else, so a shape would come out different from its design.
  const arrowheads = new Set([null, "arrow", "bar", "dot", "circle", "circle_outline", "triangle",
    "triangle_outline", "diamond", "diamond_outline", "crowfoot_one", "crowfoot_many", "crowfoot_one_or_many"]);
  const fonts = new Set([1, 2, 3, 5, 6, 7, 8, 9]);
  const dir = join(process.cwd(), "resources/libraries");
  const files = readdirSync(dir).filter((f) => f.endsWith(".excalidrawlib"));
  assert.ok(files.includes("kanagawa-erd-wave.excalidrawlib"));
  for (const file of files) {
    const lib = JSON.parse(readFileSync(join(dir, file), "utf8"));
    assert.equal(lib.type, "excalidrawlib", file);
    assert.equal(lib.version, 2, file);
    const itemIds = lib.libraryItems.map((item) => item.id);
    assert.equal(new Set(itemIds).size, itemIds.length, `${file}: item ids must be unique`);
    for (const item of lib.libraryItems) {
      const where = `${file} · ${item.name}`;
      assert.ok(item.elements.length > 0, `${where}: empty`);
      const ids = new Set(item.elements.map((e) => e.id));
      assert.equal(ids.size, item.elements.length, `${where}: element ids must be unique`);
      for (const e of item.elements) {
        for (const key of ["id", "type", "x", "y", "width", "height"]) {
          assert.ok(e[key] !== undefined, `${where}: ${e.id ?? "?"} has no ${key}`);
        }
        if (e.type === "text") assert.ok(fonts.has(e.fontFamily), `${where}: font ${e.fontFamily}`);
        if (e.type === "text" && e.containerId) {
          const container = item.elements.find((c) => c.id === e.containerId);
          assert.ok(container, `${where}: label bound to an element outside the item`);
          assert.ok(container.boundElements?.some((b) => b.id === e.id), `${where}: container does not list its label`);
        }
        if (e.type === "arrow") {
          assert.ok(arrowheads.has(e.startArrowhead) && arrowheads.has(e.endArrowhead), `${where}: arrowhead`);
          for (const b of [e.startBinding, e.endBinding]) {
            if (b) assert.ok(ids.has(b.elementId), `${where}: bound to an element outside the item`);
          }
        }
      }
      // A connector (an arrow and its label) is left ungrouped, so it can be
      // selected, reshaped and bound like an arrow drawn by hand.
      const connector = item.elements.every((e) => e.type === "arrow" || (e.type === "text" && e.containerId));
      if (connector) assert.ok(item.elements.every((e) => e.groupIds.length === 0), `${where}: connector is grouped`);
    }
  }
});

console.log(`\n${checks} checks passed`);
