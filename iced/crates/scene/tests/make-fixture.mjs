/**
 * Writes fixtures/every-element.excalidraw the way Excalidraw writes a file,
 * `JSON.stringify(data, null, 2)`, so the round-trip test compares against
 * JavaScript's own formatting: its number spellings, escapes and indentation.
 *
 *   node iced/crates/scene/tests/make-fixture.mjs
 */
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const base = (id, type, extra) => ({
  id, type, x: 10.5, y: -20.25, width: 120, height: 80, angle: 0,
  strokeColor: "#1e1e1e", backgroundColor: "transparent", fillStyle: "solid",
  strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100,
  groupIds: [], frameId: null, index: "a0", roundness: null,
  seed: 1968410350, version: 3, versionNonce: 361174001, isDeleted: false,
  boundElements: null, updated: 1731312000000, link: null, locked: false,
  ...extra,
});

const data = {
  type: "excalidraw",
  version: 2,
  source: "https://excalidraw.com",
  elements: [
    base("rect", "rectangle", { roundness: { type: 3 }, backgroundColor: "#a5d8ff", fillStyle: "hachure" }),
    base("diam", "diamond", { roundness: { type: 2 }, fillStyle: "cross-hatch", backgroundColor: "#ffc9c9" }),
    base("ell", "ellipse", { strokeStyle: "dashed", angle: 0.7853981633974483 }),
    base("neg", "rectangle", { width: -40, height: -30 }),
    base("legacy-round", "rectangle", { roundness: undefined, strokeSharpness: "round" }),
    base("line", "line", { x: 0, y: 0, points: [[5, 5], [105, 45], [55, 95]], lastCommittedPoint: null, startBinding: null, endBinding: null, startArrowhead: null, endArrowhead: null }),
    base("arrow", "arrow", { points: [[0, 0], [200, 1e-7]], startArrowhead: "dot", startBinding: null, endBinding: null, elbowed: false }),
    base("arrow-null", "arrow", { points: [[0, 0], [10, 10]], endArrowhead: null }),
    base("arrow-old", "arrow", { points: [[0, 0]], width: 50, height: 25 }),
    base("draw", "freedraw", { points: [[0, 0], [1.5, 2.25], [3, 4]], pressures: [0.5, 0.6, 0.7], simulatePressure: false, lastCommittedPoint: [3, 4] }),
    base("txt", "text", { text: "Hello \"world\"\n\tcafé ✏️ 日本", fontSize: 20, fontFamily: 5, textAlign: "center", verticalAlign: "middle", containerId: null, originalText: "Hello \"world\"\n\tcafé ✏️ 日本", autoResize: true, lineHeight: 1.25 }),
    base("txt-legacy", "text", { text: "a\nb", font: "28px Virgil", height: 70, lineHeight: undefined }),
    base("img", "image", { status: "saved", fileId: "f1", scale: [-1, 1], crop: null }),
    base("frame", "frame", { name: "Frame 1" }),
    base("future", "hologram", { depth: 3, nested: { keep: [1, 2.5, "x", null, true] } }),
    base("gone", "rectangle", { isDeleted: true }),
  ],
  appState: { gridSize: 20, gridStep: 5, gridModeEnabled: false, viewBackgroundColor: "#ffffff", lockedMultiSelections: {} },
  files: {
    f1: { mimeType: "image/png", id: "f1", dataURL: "data:image/png;base64,iVBORw0KGgo=", created: 1731312000000, lastRetrieved: 1731312000000 },
  },
  extraTopLevel: { big: 1e21, small: 5e-324, neg: -0, max: 1.7976931348623157e308 },
};

const here = dirname(fileURLToPath(import.meta.url));
writeFileSync(join(here, "fixtures/every-element.excalidraw"), JSON.stringify(data, null, 2));
