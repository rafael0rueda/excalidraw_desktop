/**
 * Writes freehand.json: Excalidraw's freedraw outline path (perfect-freehand
 * 1.2.0 through `getFreeDrawSvgPath`) for a spread of strokes, for
 * tests/freehand.rs to compare with the Rust port, string for string.
 *
 *   node iced/crates/render/tests/gen-freehand.mjs
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../../..");
const { getStroke } = await import(join(root, "node_modules/perfect-freehand/dist/esm/index.js"));
const version = JSON.parse(readFileSync(join(root, "node_modules/perfect-freehand/package.json"), "utf8")).version;

// Excalidraw 0.18.1's getFreeDrawSvgPath and getSvgPathFromStroke, verbatim
// but for `last`, which restoreElement always nulls.
const med = (A, B) => [(A[0] + B[0]) / 2, (A[1] + B[1]) / 2];
const TO_FIXED_PRECISION = /(\s?[A-Z]?,?-?[0-9]*\.[0-9]{0,2})(([0-9]|e|-)*)/g;
const getSvgPathFromStroke = (points) => {
  if (!points.length) return "";
  const max = points.length - 1;
  return points
    .reduce((acc, point, i, arr) => {
      if (i === max) acc.push(point, med(point, arr[0]), "L", arr[0], "Z");
      else acc.push(point, med(point, arr[i + 1]));
      return acc;
    }, ["M", points[0], "Q"])
    .join(" ")
    .replace(TO_FIXED_PRECISION, "$1");
};
const getFreeDrawSvgPath = (element) => {
  const inputPoints = element.simulatePressure
    ? element.points
    : element.points.length
      ? element.points.map(([x, y], i) => [x, y, element.pressures[i]])
      : [[0, 0, 0.5]];
  const options = {
    simulatePressure: element.simulatePressure,
    size: element.strokeWidth * 4.25,
    thinning: 0.6,
    smoothing: 0.5,
    streamline: 0.5,
    easing: (t) => Math.sin((t * Math.PI) / 2),
    last: false,
  };
  return getSvgPathFromStroke(getStroke(inputPoints, options));
};

let state = 424242;
const rand = () => {
  state |= 0;
  state = (state + 0x6d2b79f5) | 0;
  let t = Math.imul(state ^ (state >>> 15), 1 | state);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

const cases = [];
for (let c = 0; c < 60; c++) {
  const n = [0, 1, 2, 3, 5, 20, 80, 200][c % 8];
  const simulatePressure = c % 3 !== 0;
  const strokeWidth = [1, 2, 4][c % 3];
  const points = [];
  let x = 0, y = 0, a = rand() * 6.28;
  for (let i = 0; i < n; i++) {
    points.push([x, y]);
    // Mostly smooth, sometimes a sharp turn back.
    a += rand() < 0.08 ? Math.PI * (0.8 + rand() * 0.4) : (rand() - 0.5) * 0.6;
    const step = rand() * 6;
    x += Math.cos(a) * step;
    y += Math.sin(a) * step;
  }
  const pressures = simulatePressure ? [] : points.map(() => (rand() < 0.05 ? -1 : rand()));
  // A stroke back onto its start: a loop.
  if (c % 7 === 0 && points.length > 3) points.push([...points[0]]), pressures.length && pressures.push(0.5);
  const element = { points, pressures, simulatePressure, strokeWidth };
  cases.push({ ...element, path: getFreeDrawSvgPath(element) });
}
writeFileSync(join(here, "freehand.json"), JSON.stringify({ version, cases }) + "\n");
console.log(`perfect-freehand ${version}: ${cases.length} cases`);
