/**
 * Writes the drawings the phase 1 gate is measured on, into iced/target/bench:
 *
 * - showcase.excalidraw: one of everything the viewer draws (every shape,
 *   fill and stroke style, arrowheads, every font, freedraw, an image, a
 *   frame), for comparing the two renderers side by side;
 * - small, 2k and 10k: diagrams of that many elements (boxes with labels,
 *   arrows between them, scribbles), for the timings.
 *
 *   node iced/bench/gen-scenes.mjs
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, "../target/bench");
mkdirSync(out, { recursive: true });

let state = 2026;
const rand = () => {
  state |= 0;
  state = (state + 0x6d2b79f5) | 0;
  let t = Math.imul(state ^ (state >>> 15), 1 | state);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (xs) => xs[Math.floor(rand() * xs.length)];

let n = 0;
const el = (type, x, y, extra = {}) => ({
  id: `el${n++}`, type, x, y, width: 0, height: 0, angle: 0,
  strokeColor: "#1e1e1e", backgroundColor: "transparent", fillStyle: "solid",
  strokeWidth: 2, strokeStyle: "solid", roughness: 1, opacity: 100,
  groupIds: [], frameId: null, roundness: null,
  seed: Math.floor(rand() * 2 ** 31), version: 1, versionNonce: 0, isDeleted: false,
  boundElements: null, updated: 1, link: null, locked: false, ...extra,
});
const label = (x, y, w, text, extra = {}) =>
  el("text", x, y, { width: w, height: 25, text, originalText: text, fontSize: 20, fontFamily: 5, textAlign: "center", verticalAlign: "middle", containerId: null, autoResize: true, lineHeight: 1.25, ...extra });
const scribble = (x, y, len) => {
  const pts = [[0, 0]];
  let px = 0, py = 0, a = rand() * 6.28;
  for (let i = 0; i < len; i++) {
    a += (rand() - 0.5) * 0.7;
    px += Math.cos(a) * 3;
    py += Math.sin(a) * 3;
    pts.push([px, py]);
  }
  return el("freedraw", x, y, { points: pts, pressures: [], simulatePressure: true, lastCommittedPoint: null });
};

/** A tiny PNG: a 64×64 gradient, so the image path is exercised without a binary fixture. */
const png = () => {
  const w = 64, h = 64;
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0;
    for (let x = 0; x < w; x++) {
      const o = y * (w * 4 + 1) + 1 + x * 4;
      raw[o] = x * 4; raw[o + 1] = y * 4; raw[o + 2] = 160; raw[o + 3] = 255;
    }
  }
  const crcTable = Array.from({ length: 256 }, (_, c) => { for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc = (buf) => { let c = 0xffffffff; for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const c = Buffer.alloc(4); c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]).toString("base64");
};

const file = (elements, files = {}) =>
  JSON.stringify({ type: "excalidraw", version: 2, source: "bench", elements, appState: { viewBackgroundColor: "#ffffff", gridSize: 20 }, files }, null, 2);

// --- showcase
{
  n = 0;
  const els = [];
  const fills = ["hachure", "cross-hatch", "zigzag", "solid"];
  const strokes = ["solid", "dashed", "dotted"];
  fills.forEach((fillStyle, i) => {
    strokes.forEach((strokeStyle, j) => {
      const x = i * 190, y = j * 140;
      const type = ["rectangle", "ellipse", "diamond"][j];
      els.push(el(type, x, y, { width: 160, height: 110, fillStyle, strokeStyle, backgroundColor: ["#a5d8ff", "#ffc9c9", "#b2f2bb", "#ffec99"][i], roundness: type === "ellipse" ? null : { type: type === "rectangle" ? 3 : 2 }, roughness: [0, 1, 2][(i + j) % 3] }));
    });
  });
  const heads = ["arrow", "bar", "dot", "circle_outline", "triangle", "triangle_outline", "diamond", "crowfoot_many", "crowfoot_one_or_many"];
  heads.forEach((h, i) => {
    els.push(el("arrow", 800, i * 45, { width: 220, height: 20, points: [[0, 0], [110, i % 2 ? 20 : -20], [220, 0]], roundness: { type: 2 }, startArrowhead: i % 3 === 0 ? "dot" : null, endArrowhead: h, strokeColor: "#e03131" }));
  });
  els.push(el("arrow", 800, 430, { elbowed: true, points: [[0, 0], [120, 0], [120, 80], [220, 80]], width: 220, height: 80, endArrowhead: "arrow", strokeColor: "#1971c2" }));
  els.push(el("line", 1080, 0, { points: [[0, 0], [120, 30], [60, 140], [0, 0]], width: 120, height: 140, backgroundColor: "#d0bfff", fillStyle: "hachure" }));
  const fonts = [[5, "Excalifont"], [1, "Virgil"], [6, "Nunito"], [7, "Lilita One"], [8, "Comic Shanns"], [9, "Liberation Sans"], [3, "Cascadia"]];
  fonts.forEach(([family, name], i) => {
    const text = `${name}: The quick brown fox — ¿Qué tal? 0123`;
    els.push(label(0, 470 + i * 40, 600, text, { fontFamily: family, textAlign: "left", lineHeight: family === 6 ? 1.35 : family === 7 || family === 9 ? 1.15 : family === 3 ? 1.2 : 1.25 }));
  });
  els.push(label(650, 520, 300, "Two lines,\ncentred", { textAlign: "center", height: 50, angle: -0.2, strokeColor: "#2f9e44" }));
  for (let i = 0; i < 4; i++) els.push(scribble(1050 + i * 40, 250 + i * 30, 40 + i * 30));
  const loop = scribble(1100, 450, 60);
  loop.points.push([0, 0]);
  loop.backgroundColor = "#ffec99"; loop.fillStyle = "cross-hatch";
  els.push(loop);
  els.push(el("image", 650, 600, { width: 128, height: 96, fileId: "img1", status: "saved", scale: [1, 1], crop: null }));
  const frame = el("frame", 1050, 600, { width: 260, height: 160, name: "Frame 1" });
  els.push(frame);
  els.push(el("ellipse", 1220, 680, { width: 160, height: 120, frameId: frame.id, backgroundColor: "#ffc9c9", fillStyle: "solid" }));
  writeFileSync(join(out, "showcase.excalidraw"), file(els, { img1: { mimeType: "image/png", id: "img1", dataURL: `data:image/png;base64,${png()}`, created: 1, lastRetrieved: 1 } }));
}

// --- diagrams
const diagram = (count) => {
  n = 0;
  const els = [];
  const cols = Math.ceil(Math.sqrt(count / 3));
  for (let i = 0; els.length < count; i++) {
    const x = (i % cols) * 260, y = Math.floor(i / cols) * 200;
    const type = pick(["rectangle", "rectangle", "ellipse", "diamond"]);
    els.push(el(type, x, y, { width: 160, height: 90, backgroundColor: pick(["transparent", "#a5d8ff", "#ffc9c9", "#b2f2bb"]), fillStyle: pick(["hachure", "solid", "cross-hatch"]), roundness: type === "rectangle" && rand() < 0.6 ? { type: 3 } : null }));
    els.push(label(x + 10, y + 32, 140, `Node ${i}`));
    if (rand() < 0.8) {
      els.push(el("arrow", x + 165, y + 45, { points: [[0, 0], [85, rand() * 40 - 20]], width: 85, height: 20, endArrowhead: "arrow", roundness: { type: 2 } }));
    } else {
      els.push(scribble(x + 170, y + 20, 30));
    }
  }
  return els.slice(0, count);
};
for (const [name, count] of [["small", 60], ["2k", 2000], ["10k", 10000]]) {
  writeFileSync(join(out, `${name}.excalidraw`), file(diagram(count)));
}
console.log(`wrote ${out}/{showcase,small,2k,10k}.excalidraw`);
