/**
 * Writes excalidraw-shapes.digest.json: what Excalidraw 0.18.1 itself makes of
 * a spread of elements, for tests/excalidraw_shapes.rs. For each element:
 *
 * - the view `restoreElements` builds (position, size, style), which the
 *   scene crate must agree with;
 * - every rough.js drawable `ShapeCache.generateElementShape` returns, as its
 *   shape, drawing options, and a hash of every number in every op set;
 * - for freedraw, `getFreeDrawSvgPath`.
 *
 * Excalidraw's bundle expects a browser, so it is bundled with esbuild (a
 * Vite dependency, already installed) behind a few stubbed globals.
 *
 *   node iced/crates/render/tests/gen-excalidraw-shapes.mjs
 */
import { build } from "esbuild";
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../../..");
const pkg = join(root, "node_modules/@excalidraw/excalidraw");
const version = JSON.parse(readFileSync(join(pkg, "package.json"), "utf8")).version;

const stubs = `
const noop = () => {};
const el = () => ({ style: {}, getContext: () => null, setAttribute: noop, appendChild: noop, addEventListener: noop, classList: { add: noop, remove: noop } });
globalThis.window = globalThis;
globalThis.document = { createElement: el, body: el(), documentElement: el(), addEventListener: noop, fonts: { add: noop, check: () => true, load: async () => [], ready: Promise.resolve() }, querySelector: () => null };
globalThis.devicePixelRatio = 1;
globalThis.FontFace = class { constructor() { this.status = "loaded"; } load() { return Promise.resolve(this); } };
globalThis.Path2D = class { constructor(d) { this.d = d; } };
globalThis.addEventListener = noop;
globalThis.location = { href: "http://localhost/", origin: "http://localhost" };
globalThis.matchMedia = () => ({ matches: false, addEventListener: noop });
globalThis.localStorage = { getItem: () => null, setItem: noop };
`;
const dir = mkdtempSync(join(tmpdir(), "excalidraw-shapes-"));
writeFileSync(join(dir, "stubs.mjs"), stubs);
writeFileSync(
  join(dir, "entry.mjs"),
  `import "./stubs.mjs";\nexport { ShapeCache, restoreElements, getFreeDrawSvgPath } from ${JSON.stringify(join(pkg, "dist/dev/chunk-4FTI6OG3.js"))};\n`,
);
await build({
  entryPoints: [join(dir, "entry.mjs")],
  bundle: true,
  platform: "node",
  format: "esm",
  outfile: join(dir, "out.mjs"),
  logLevel: "error",
  loader: { ".css": "empty", ".scss": "empty" },
});
const { ShapeCache, restoreElements, getFreeDrawSvgPath } = await import(pathToFileURL(join(dir, "out.mjs")).href);
rmSync(dir, { recursive: true, force: true });

let state = 77;
const rand = () => {
  state |= 0;
  state = (state + 0x6d2b79f5) | 0;
  let t = Math.imul(state ^ (state >>> 15), 1 | state);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (xs) => xs[Math.floor(rand() * xs.length)];
const num = (lo, hi) => lo + rand() * (hi - lo);

const style = () => ({
  seed: Math.floor(rand() * 2 ** 31),
  strokeColor: pick(["#1e1e1e", "#e03131", "#1971c2"]),
  backgroundColor: pick(["transparent", "transparent", "#a5d8ff", "#ffc9c9", "#ffffff00"]),
  fillStyle: pick(["hachure", "cross-hatch", "solid", "zigzag"]),
  strokeWidth: pick([1, 2, 4]),
  strokeStyle: pick(["solid", "solid", "dashed", "dotted"]),
  roughness: pick([0, 1, 2]),
  opacity: 100,
});
const ARROWHEADS = [null, "arrow", "bar", "dot", "circle", "circle_outline", "triangle", "triangle_outline", "diamond", "diamond_outline", "crowfoot_one", "crowfoot_many", "crowfoot_one_or_many"];

const inputs = [];
let id = 0;
const add = (type, extra) => inputs.push({ id: `e${id++}`, type, x: num(-100, 100), y: num(-100, 100), angle: 0, ...style(), ...extra });

for (let i = 0; i < 24; i++) {
  const size = () => pick([num(4, 14), num(14, 60), num(60, 500)]);
  const roundness = pick([null, { type: 1 }, { type: 2 }, { type: 3 }, { type: 3, value: 12 }]);
  add("rectangle", { width: size(), height: size(), roundness });
  add("diamond", { width: size(), height: size(), roundness: pick([null, { type: 2 }]) });
  add("ellipse", { width: size(), height: size() });
}
for (let i = 0; i < 6; i++) {
  add("embeddable", { width: 300, height: 200, roundness: null, strokeColor: pick(["transparent", "#1e1e1e"]), backgroundColor: "transparent" });
  add("iframe", { width: 300, height: 200, roundness: { type: 3 }, strokeColor: "transparent", backgroundColor: pick(["transparent", "#a5d8ff"]) });
}
const path = (n, scale) => {
  const pts = [[0, 0]];
  for (let i = 1; i < n; i++) pts.push([num(-scale, scale), num(-scale, scale)]);
  return pts;
};
for (const start of ARROWHEADS) {
  for (const end of [null, "arrow", pick(ARROWHEADS)]) {
    add("arrow", { points: path(pick([2, 2, 3, 5]), pick([20, 200])), startArrowhead: start, endArrowhead: end, roundness: pick([null, { type: 2 }]) });
  }
}
// Arrows the way older files have them: points off the origin, no arrowhead keys.
add("arrow", { points: [[10, 10], [150, 60]] });
add("arrow", { points: [[0, 0], [0, 0]] });
for (let i = 0; i < 6; i++) {
  const pts = [[0, 0], [num(50, 200), 0], [num(50, 200), num(50, 200)], [num(200, 300), num(50, 200)]];
  add("arrow", { elbowed: true, points: pts, roundness: null, endArrowhead: "arrow" });
}
for (let i = 0; i < 12; i++) {
  const pts = path(pick([2, 3, 6]), 150);
  if (i % 3 === 0) pts.push([0.5, 1]); // a loop
  add("line", { points: pts, roundness: pick([null, { type: 2 }]) });
}
for (let i = 0; i < 10; i++) {
  const pts = [[0, 0]];
  let x = 0, y = 0, a = 0;
  for (let k = 0; k < [3, 30, 90][i % 3]; k++) {
    a += (rand() - 0.5) * 0.8;
    x += Math.cos(a) * 4;
    y += Math.sin(a) * 4;
    pts.push([x, y]);
  }
  if (i % 2 === 0) pts.push([1, 1]); // a loop
  const simulatePressure = i % 4 !== 0;
  add("freedraw", { points: pts, simulatePressure, pressures: simulatePressure ? [] : pts.map(() => rand()) });
}

const KIND = { move: 0, lineTo: 1, bcurveTo: 2 };
const fnv = (ops) => {
  let h = 0xcbf29ce484222325n;
  const step = (byte) => {
    h ^= BigInt(byte);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  };
  const view = new DataView(new ArrayBuffer(8));
  for (const op of ops) {
    step(KIND[op.op]);
    for (const n of op.data) {
      view.setFloat64(0, n, true);
      for (let i = 0; i < 8; i++) step(view.getUint8(i));
    }
  }
  return h.toString(16).padStart(16, "0");
};

const CANVAS = "#fafafa";
const restored = restoreElements(structuredClone(inputs), null);
const cases = inputs.map((input) => {
  const e = restored.find((r) => r.id === input.id);
  const generated = ShapeCache.generateElementShape(e, { isExporting: false, canvasBackgroundColor: CANVAS, embedsValidationStatus: null });
  const drawables = generated == null ? [] : Array.isArray(generated) ? generated : [generated];
  return {
    input,
    view: { x: e.x, y: e.y, width: e.width, height: e.height, roughness: e.roughness, strokeWidth: e.strokeWidth, isDeleted: e.isDeleted },
    drawables: drawables.map((d) => ({
      shape: d.shape,
      stroke: d.options.stroke,
      strokeWidth: d.options.strokeWidth,
      fill: d.options.fill ?? null,
      fillStyle: d.options.fillStyle,
      strokeLineDash: d.options.strokeLineDash ?? null,
      roughness: d.options.roughness,
      sets: d.sets.map((s) => ({ type: s.type, n: s.ops.length, fnv: fnv(s.ops) })),
    })),
    freedrawPath: e.type === "freedraw" ? getFreeDrawSvgPath(e) : null,
  };
});
writeFileSync(join(here, "excalidraw-shapes.digest.json"), JSON.stringify({ version, canvas: CANVAS, cases }) + "\n");
console.log(`Excalidraw ${version}: ${cases.length} elements`);
