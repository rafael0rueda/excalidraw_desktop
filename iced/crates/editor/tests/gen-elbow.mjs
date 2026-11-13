/**
 * Writes elbow.json: Excalidraw 0.18.1's own elbow arrow routes
 * (`updateElbowArrowPoints`), for tests/elbow.rs. Three kinds of case:
 *
 * - unbound arrows between two points;
 * - arrows bound at both ends to shapes, by `fixedPoint`, as when a bound
 *   shape moves (rectangles at exactly the binding distance; ellipses,
 *   diamonds and rotated shapes as Excalidraw computes the point);
 * - arrows being drawn, their end over a shape (`isDragging`), with the
 *   shapes Excalidraw finds under each end recorded.
 *
 * Bundled as gen-excalidraw-shapes.mjs is, behind the same stubs.
 *
 *   node iced/crates/editor/tests/gen-elbow.mjs
 */
import { build } from "esbuild";
import { writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../../..");
const pkg = join(root, "node_modules/@excalidraw/excalidraw");

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
const dir = mkdtempSync(join(tmpdir(), "excalidraw-elbow-"));
writeFileSync(join(dir, "stubs.mjs"), stubs);
writeFileSync(
  join(dir, "entry.mjs"),
  `import "./stubs.mjs";\nexport { updateElbowArrowPoints, restoreElements, arrayToMap, calculateFixedPointForElbowArrowBinding, getHoveredElementForBinding } from ${JSON.stringify(join(pkg, "dist/dev/chunk-4FTI6OG3.js"))};\n`,
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
const X = await import(pathToFileURL(join(dir, "out.mjs")).href);
rmSync(dir, { recursive: true, force: true });

let state = 9;
const rand = () => {
  state |= 0;
  state = (state + 0x6d2b79f5) | 0;
  let t = Math.imul(state ^ (state >>> 15), 1 | state);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (xs) => xs[Math.floor(rand() * xs.length)];
const num = (lo, hi) => Math.round(lo + rand() * (hi - lo));

const base = (id, type, x, y, w, h, angle = 0) => ({
  id, type, x, y, width: w, height: h, angle, strokeColor: "#1e1e1e", backgroundColor: "transparent", fillStyle: "solid",
  strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100, groupIds: [], frameId: null, roundness: null,
  seed: 1, version: 1, versionNonce: 0, isDeleted: false, boundElements: [], updated: 1, link: null, locked: false,
});
const arrowRaw = (x, y, dx, dy, startArrowhead, endArrowhead) => ({
  ...base("arrow", "arrow", x, y, Math.abs(dx), Math.abs(dy)),
  points: [[0, 0], [dx, dy]], lastCommittedPoint: null, startBinding: null, endBinding: null,
  startArrowhead, endArrowhead, elbowed: true, fixedSegments: null, startIsSpecial: null, endIsSpecial: null,
});
const restore = (els) => X.restoreElements(els, null, { refreshDimensions: false, repairBindings: false });
const route = (arrow, map, updates, options) => {
  const r = X.updateElbowArrowPoints(arrow, map, updates, options);
  return { x: r.x, y: r.y, points: r.points.map((p) => [p[0], p[1]]) };
};

const cases = [];

// Unbound.
for (let i = 0; i < 40; i++) {
  const [x, y] = [num(-300, 300), num(-300, 300)];
  const [dx, dy] = [num(-400, 400), num(-400, 400)];
  const heads = [pick([null, "arrow"]), pick([null, "arrow", "triangle"])];
  const [arrow] = restore([arrowRaw(x, y, dx, dy, ...heads)]);
  cases.push({ kind: "free", elements: [], arrow, start: null, end: null, dragging: false, result: route(arrow, new Map(), { points: [[0, 0], [dx, dy]] }) });
}

// Bound at both ends.
const shape = (id, type, rotated) => base(id, type, num(-600, 600), num(-600, 600), num(40, 260), num(40, 200), rotated ? pick([0, 0.3, 1.1, 2.5]) : 0);
for (let i = 0; i < 60; i++) {
  const types = ["rectangle", "rectangle", "ellipse", "diamond"];
  const exact = i < 30;
  const [a, b] = restore([shape("a", exact ? "rectangle" : pick(types), !exact && rand() < 0.4), shape("b", exact ? "rectangle" : pick(types), !exact && rand() < 0.4)]);
  // A fixed point at the binding distance from a side, for the exact cases.
  const sideFixed = (e) => {
    const t = 0.1 + rand() * 0.8;
    const d = 5;
    return pick([
      [(e.width + d) / e.width, t],
      [-d / e.width, t],
      [t, (e.height + d) / e.height],
      [t, -d / e.height],
    ]);
  };
  const [sx, sy] = [a.x + a.width / 2, a.y + a.height / 2];
  const [ex, ey] = [b.x + b.width / 2, b.y + b.height / 2];
  let [arrow] = restore([arrowRaw(sx, sy, ex - sx, ey - sy, null, pick([null, "arrow"]))]);
  const map = X.arrayToMap([a, b, arrow]);
  const fixed = (e, end) => (exact ? sideFixed(e) : X.calculateFixedPointForElbowArrowBinding(arrow, e, end, map).fixedPoint);
  const startBinding = { elementId: "a", focus: 0, gap: 0, fixedPoint: fixed(a, "start") };
  const endBinding = { elementId: "b", focus: 0, gap: 0, fixedPoint: fixed(b, "end") };
  arrow = { ...arrow, startBinding, endBinding };
  map.set("arrow", arrow);
  a.boundElements = [{ id: "arrow", type: "arrow" }];
  b.boundElements = [{ id: "arrow", type: "arrow" }];
  // Copies: the same objects would read as "nothing changed".
  const result = route(arrow, map, { points: [arrow.points[0], arrow.points[arrow.points.length - 1]], startBinding: { ...startBinding }, endBinding: { ...endBinding } });
  cases.push({ kind: exact ? "bound" : "bound-approx", elements: [a, b], arrow, start: startBinding, end: endBinding, dragging: false, result });
}

// Drawn: the end over a shape, the start free or over another.
for (let i = 0; i < 30; i++) {
  const [a, b] = restore([shape("a", pick(["rectangle", "ellipse", "diamond"]), false), shape("b", pick(["rectangle", "ellipse", "diamond"]), false)]);
  const inside = (e) => [e.x + e.width * (0.3 + rand() * 0.4), e.y + e.height * (0.3 + rand() * 0.4)];
  const s = rand() < 0.5 ? inside(a) : [num(-700, 700), num(-700, 700)];
  const e = inside(b);
  const [arrow] = restore([arrowRaw(s[0], s[1], e[0] - s[0], e[1] - s[1], null, "arrow")]);
  const elements = [a, b];
  const map = X.arrayToMap([a, b, arrow]);
  const zoom = { value: 1 };
  const hovered = (p) => X.getHoveredElementForBinding({ x: p[0], y: p[1] }, elements, map, zoom, true, true)?.id ?? null;
  const result = route(arrow, map, { points: [[0, 0], [e[0] - s[0], e[1] - s[1]]] }, { isDragging: true, zoom });
  cases.push({ kind: "drawn", elements, arrow, start: null, end: null, dragging: true, hoveredStart: hovered(s), hoveredEnd: hovered(e), result });
}

writeFileSync(join(here, "elbow.json"), JSON.stringify({ excalidraw: "0.18.1", cases }));
console.log(`wrote ${cases.length} cases`);
