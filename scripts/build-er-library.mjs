// Generates resources/libraries/entity-relationship.excalidrawlib
// Run with: node scripts/build-er-library.mjs
import { writeFileSync, mkdirSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_PATH = join(__dirname, "..", "resources", "libraries", "entity-relationship.excalidrawlib");

const now = Date.now();
const rndInt = () => Math.floor(Math.random() * 2 ** 31);
function cryptoId() {
  return randomBytes(12).toString("hex");
}

const STROKE = "#1e1e1e";
const FONT_FAMILY = 5; // Excalifont

function baseProps(overrides) {
  return {
    id: cryptoId(),
    x: 0,
    y: 0,
    strokeColor: STROKE,
    backgroundColor: "transparent",
    fillStyle: "solid",
    strokeWidth: 2,
    strokeStyle: "solid",
    roundness: null,
    roughness: 1,
    opacity: 100,
    width: 100,
    height: 100,
    angle: 0,
    seed: rndInt(),
    version: 1,
    versionNonce: rndInt(),
    index: null,
    isDeleted: false,
    groupIds: [],
    frameId: null,
    boundElements: null,
    updated: now,
    link: null,
    locked: false,
    ...overrides,
  };
}

function rect(x, y, w, h, overrides = {}) {
  return baseProps({ type: "rectangle", x, y, width: w, height: h, ...overrides });
}

function ellipse(x, y, w, h, overrides = {}) {
  return baseProps({ type: "ellipse", x, y, width: w, height: h, ...overrides });
}

function diamond(x, y, w, h, overrides = {}) {
  return baseProps({ type: "diamond", x, y, width: w, height: h, ...overrides });
}

function text(x, y, w, h, str, overrides = {}) {
  return baseProps({
    type: "text",
    x,
    y,
    width: w,
    height: h,
    text: str,
    originalText: str,
    fontSize: 16,
    fontFamily: FONT_FAMILY,
    textAlign: "center",
    verticalAlign: "middle",
    containerId: null,
    autoResize: true,
    lineHeight: 1.25,
    strokeWidth: 1,
    backgroundColor: "transparent",
    ...overrides,
  });
}

function line(x, y, points, overrides = {}) {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const w = Math.max(...xs) - Math.min(...xs) || 1;
  const h = Math.max(...ys) - Math.min(...ys) || 1;
  return baseProps({
    type: "line",
    x,
    y,
    width: w,
    height: h,
    points,
    lastCommittedPoint: null,
    startBinding: null,
    endBinding: null,
    startArrowhead: null,
    endArrowhead: null,
    ...overrides,
  });
}

function grouped(elements) {
  const groupId = cryptoId();
  return elements.map((el) => ({ ...el, groupIds: [groupId, ...el.groupIds] }));
}

function libItem(name, elements) {
  return {
    id: cryptoId(),
    status: "unpublished",
    elements,
    created: now,
    name,
  };
}

const items = [];

// 1. Entity (Table) — IE / crow's-foot notation
{
  const w = 220;
  const headerH = 36;
  const rowH = 28;
  const rows = ["PK   id", "name", "FK   ref_id"];
  const els = [
    rect(0, 0, w, headerH, { backgroundColor: "#a5d8ff", fillStyle: "solid" }),
    rect(0, headerH, w, rowH * rows.length, { backgroundColor: "#ffffff", fillStyle: "solid" }),
    ...rows.slice(1).map((_, i) =>
      line(0, headerH + rowH * (i + 1), [
        [0, 0],
        [w, 0],
      ], { strokeWidth: 1 }),
    ),
    text(0, headerH / 2 - 12, w, 24, "Entity", { fontSize: 18, textAlign: "center" }),
    ...rows.map((r, i) =>
      text(10, headerH + rowH * i + rowH / 2 - 10, w - 20, 20, r, {
        fontSize: 15,
        textAlign: "left",
      }),
    ),
  ];
  items.push(libItem("ER: Entity (Table)", grouped(els)));
}

// 2. Weak Entity — double-border rectangle
{
  const els = [
    rect(0, 0, 180, 80, { backgroundColor: "#a5d8ff", fillStyle: "solid" }),
    rect(6, 6, 168, 68, { backgroundColor: "transparent" }),
    text(6, 6, 168, 68, "Weak Entity", { fontSize: 16 }),
  ];
  items.push(libItem("ER: Weak Entity", grouped(els)));
}

// 3. Relationship (diamond) — Chen notation
{
  const els = [
    diamond(0, 0, 160, 90, { backgroundColor: "#ffec99", fillStyle: "solid" }),
    text(20, 30, 120, 30, "Relationship", { fontSize: 15 }),
  ];
  items.push(libItem("ER: Relationship", grouped(els)));
}

// 4. Identifying Relationship — double-border diamond
{
  const els = [
    diamond(0, 0, 170, 96, { backgroundColor: "#ffec99", fillStyle: "solid" }),
    diamond(8, 8, 154, 80, { backgroundColor: "transparent" }),
    text(20, 33, 130, 30, "Identifying", { fontSize: 14 }),
  ];
  items.push(libItem("ER: Identifying Relationship", grouped(els)));
}

// 5. Attribute (ellipse) — Chen notation
{
  const els = [
    ellipse(0, 0, 140, 70, { backgroundColor: "#d3f9d8", fillStyle: "solid" }),
    text(10, 22, 120, 26, "attribute", { fontSize: 15 }),
  ];
  items.push(libItem("ER: Attribute", grouped(els)));
}

// 6. Key Attribute — underlined text in ellipse
{
  const els = [
    ellipse(0, 0, 140, 70, { backgroundColor: "#d3f9d8", fillStyle: "solid" }),
    text(10, 20, 120, 26, "key attribute", { fontSize: 14 }),
    line(40, 47, [
      [0, 0],
      [60, 0],
    ], { strokeWidth: 1 }),
  ];
  items.push(libItem("ER: Key Attribute", grouped(els)));
}

// 7. Multivalued Attribute — double-border ellipse
{
  const els = [
    ellipse(0, 0, 150, 76, { backgroundColor: "#d3f9d8", fillStyle: "solid" }),
    ellipse(6, 6, 138, 64, { backgroundColor: "transparent" }),
    text(10, 25, 130, 26, "attributes", { fontSize: 14 }),
  ];
  items.push(libItem("ER: Multivalued Attribute", grouped(els)));
}

// 8. Derived Attribute — dashed ellipse
{
  const els = [
    ellipse(0, 0, 140, 70, { backgroundColor: "#d3f9d8", fillStyle: "solid", strokeStyle: "dashed" }),
    text(10, 22, 120, 26, "derived", { fontSize: 15 }),
  ];
  items.push(libItem("ER: Derived Attribute", grouped(els)));
}

// 9. Connector line — plain, for entity–relationship–attribute links (Chen)
{
  const els = [
    line(0, 0, [
      [0, 0],
      [140, 0],
    ]),
  ];
  items.push(libItem("ER: Connector Line", els));
}

// 10. Identifying connector — double line (Chen, links to weak entity)
{
  const els = [
    line(0, 0, [
      [0, 0],
      [140, 0],
    ]),
    line(0, 6, [
      [0, 0],
      [140, 0],
    ]),
  ];
  items.push(libItem("ER: Identifying Connector", grouped(els)));
}

// 11–15: crow's-foot / IE relationship lines
const crowfoot = [
  ["ER: One-to-One", "crowfoot_one", "crowfoot_one", "solid"],
  ["ER: One-to-Many", "crowfoot_one", "crowfoot_many", "solid"],
  ["ER: Many-to-Many", "crowfoot_many", "crowfoot_many", "solid"],
  ["ER: One-to-One-or-Many", "crowfoot_one", "crowfoot_one_or_many", "solid"],
  ["ER: Non-identifying (dashed) One-to-Many", "crowfoot_one", "crowfoot_many", "dashed"],
];
for (const [name, startArrowhead, endArrowhead, strokeStyle] of crowfoot) {
  const els = [
    line(
      0,
      0,
      [
        [0, 0],
        [160, 0],
      ],
      { startArrowhead, endArrowhead, strokeStyle },
    ),
  ];
  items.push(libItem(name, els));
}

const lib = {
  type: "excalidrawlib",
  version: 2,
  source: "https://github.com/rafael0rueda/excalidraw_desktop",
  libraryItems: items,
};

mkdirSync(dirname(OUT_PATH), { recursive: true });
writeFileSync(OUT_PATH, JSON.stringify(lib, null, 2) + "\n");
console.log(`Wrote ${items.length} library items to ${OUT_PATH}`);
