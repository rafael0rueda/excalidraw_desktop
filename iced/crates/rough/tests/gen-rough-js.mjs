/**
 * Writes what rough.js itself draws for a spread of shapes and options, for
 * tests/rough_js.rs to hold the Rust port to: rough-js.digest.json (committed;
 * a hash of every number) and rough-js.json (not committed; the numbers).
 *
 *   node iced/crates/rough/tests/gen-rough-js.mjs
 *
 * Uses the rough.js the app ships (node_modules/roughjs, 4.6.4 at the time of
 * writing). The cases are shaped like Excalidraw's calls in
 * `_generateElementShape`: its seeds, roughness values, fill styles and the
 * path strings it builds for rounded rectangles and diamonds.
 */
import { writeFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../../..");
// The bundled build: `bin/` uses extensionless imports Node will not resolve.
const rough = (await import(join(root, "node_modules/roughjs/bundled/rough.esm.js"))).default;
const version = JSON.parse(readFileSync(join(root, "node_modules/roughjs/package.json"), "utf8")).version;

// mulberry32: the case list itself must not change between runs.
let state = 20261111;
const rand = () => {
  state |= 0;
  state = (state + 0x6d2b79f5) | 0;
  let t = Math.imul(state ^ (state >>> 15), 1 | state);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (xs) => xs[Math.floor(rand() * xs.length)];
const num = (lo, hi) => lo + rand() * (hi - lo);
const seed = () => Math.floor(rand() * 2 ** 31);

/** Options as Excalidraw's `generateRoughOptions` builds them. */
const excalidrawOptions = () => {
  const strokeWidth = pick([1, 2, 4]);
  const strokeStyle = pick(["solid", "solid", "dashed", "dotted"]);
  const roughness = pick([0, 0, 1, 1, 2, 0.5, 1 / 3]);
  const o = {
    seed: seed(),
    disableMultiStroke: strokeStyle !== "solid",
    strokeWidth: strokeStyle !== "solid" ? strokeWidth + 0.5 : strokeWidth,
    fillWeight: strokeWidth / 2,
    hachureGap: strokeWidth * 4,
    roughness,
    stroke: "#1e1e1e",
    preserveVertices: roughness < 2,
  };
  if (rand() < 0.6) {
    o.fillStyle = pick(["hachure", "cross-hatch", "zigzag", "solid"]);
    o.fill = "#a5d8ff";
  }
  return o;
};

const points = (n, scale) => Array.from({ length: n }, () => [num(-scale, scale), num(-scale, scale)]);

const roundedRect = (w, h, r) =>
  `M ${r} 0 L ${w - r} 0 Q ${w} 0, ${w} ${r} L ${w} ${h - r} Q ${w} ${h}, ${w - r} ${h} L ${r} ${h} Q 0 ${h}, 0 ${h - r} L 0 ${r} Q 0 0, ${r} 0`;

const roundedDiamond = (w, h) => {
  const topX = Math.floor(w / 2) + 1, topY = 0, rightX = w, rightY = Math.floor(h / 2) + 1;
  const bottomX = topX, bottomY = h, leftX = 0, leftY = rightY;
  const v = Math.abs(topX - leftX) * 0.25, hr = Math.abs(rightY - topY) * 0.25;
  return `M ${topX + v} ${topY + hr} L ${rightX - v} ${rightY - hr}
            C ${rightX} ${rightY}, ${rightX} ${rightY}, ${rightX - v} ${rightY + hr}
            L ${bottomX + v} ${bottomY - hr}
            C ${bottomX} ${bottomY}, ${bottomX} ${bottomY}, ${bottomX - v} ${bottomY - hr}
            L ${leftX + v} ${leftY + hr}
            C ${leftX} ${leftY}, ${leftX} ${leftY}, ${leftX + v} ${leftY - hr}
            L ${topX - v} ${topY + hr}
            C ${topX} ${topY}, ${topX} ${topY}, ${topX + v} ${topY + hr}`;
};

const cases = [];
const add = (method, args, options) => cases.push({ method, args, options });

for (let i = 0; i < 40; i++) {
  const w = pick([num(5, 60), num(60, 400), num(400, 900)]);
  const h = pick([num(5, 60), num(60, 400)]);
  add("rectangle", [0, 0, w, h], excalidrawOptions());
  add("ellipse", [w / 2, h / 2, w, h], { ...excalidrawOptions(), curveFitting: 1 });
  add("polygon", [[[Math.floor(w / 2) + 1, 0], [w, Math.floor(h / 2) + 1], [Math.floor(w / 2) + 1, h], [0, Math.floor(h / 2) + 1]]], excalidrawOptions());
  add("path", [roundedRect(w, h, Math.min(w, h) * 0.25)], { ...excalidrawOptions(), preserveVertices: true });
  add("path", [roundedDiamond(w, h)], { ...excalidrawOptions(), preserveVertices: true });
}
for (let i = 0; i < 30; i++) {
  const n = pick([2, 3, 4, 7, 12]);
  const o = excalidrawOptions();
  add("linearPath", [points(n, pick([20, 300, 800]))], o);
  add("curve", [points(n, pick([20, 300]))], excalidrawOptions());
  add("line", [num(-50, 50), num(-50, 50), num(-700, 700), num(-700, 700)], { ...excalidrawOptions(), roughness: Math.min(1, pick([0, 1, 2])) });
  add("circle", [num(-50, 50), num(-50, 50), num(2, 30)], { ...excalidrawOptions(), fill: "#1e1e1e", fillStyle: "solid", roughness: Math.min(0.5, pick([0, 1, 2])) });
}
// A closed line with a fill: a polygon over the element's own points.
for (let i = 0; i < 10; i++) {
  const pts = points(pick([4, 6, 9]), 200);
  pts.push([...pts[0]]);
  add("polygon", [pts], { ...excalidrawOptions(), fill: "#ffc9c9", fillStyle: pick(["hachure", "cross-hatch", "zigzag"]) });
  add("curve", [pts], { ...excalidrawOptions(), fill: "#ffc9c9", fillStyle: pick(["hachure", "cross-hatch", "solid"]) });
}
// Path syntax Excalidraw does not write, but rough.js accepts.
add("path", ["M10 10 h 50 v 30 h -50 Z"], { seed: 7, fill: "#000", fillStyle: "hachure" });
add("path", ["M 10,10 a 20,10 30 1,1 40,20 T 80 80 S 90 90 100 100 z"], { seed: 8 });
add("path", ["m5 5 l 10 0 l 0 10 z m 20 20 c 5 0 10 5 10 10 q 5 5 10 0"], { seed: 9, fill: "#000", fillStyle: "solid" });

/**
 * FNV-1a over each op's kind and its numbers' IEEE 754 bits, little-endian.
 * tests/rough_js.rs computes the same, so the committed digest checks every
 * bit without committing megabytes of numbers.
 */
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

const generator = rough.generator();
const full = [];
const digest = [];
for (const { method, args, options } of cases) {
  // A copy: hachure fills rotate the caller's points in place and back, and
  // what that leaves behind must not become the recorded input.
  const d = generator[method](...structuredClone(args), options);
  full.push({ method, args, options, sets: d.sets.map((s) => ({ type: s.type, ops: s.ops.map((op) => [op.op, ...op.data]) })) });
  digest.push({ method, args, options, sets: d.sets.map((s) => ({ type: s.type, n: s.ops.length, fnv: fnv(s.ops) })) });
}
// Committed: what the test checks against.
writeFileSync(join(here, "rough-js.digest.json"), JSON.stringify({ version, cases: digest }) + "\n");
// Not committed (3 MB): every number, so a failure can say which one differs.
writeFileSync(join(here, "rough-js.json"), JSON.stringify({ version, cases: full }) + "\n");
console.log(`rough.js ${version}: ${cases.length} cases`);
