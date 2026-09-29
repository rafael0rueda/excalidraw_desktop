// Generates resources/libraries/kanagawa-erd-{wave,dragon,lotus}.excalidrawlib
// Run with: node scripts/build-kanagawa-erd-library.mjs
//
// The shapes come from scripts/kanagawa-erd-library.js, the source of the
// "Kanagawa ERD" Claude Design handoff. It differs from the handoff in one
// place: the crow's-foot connectors are plain arrows with bound labels rather
// than groups, and optional ends are dashed rather than marked with a circle.
// A revised design dropped over it has to keep that, and check.mjs fails if it
// does not. It is a browser-style script rather than a module, so it runs in a
// vm context. Its ids and seeds are drawn from a
// seeded generator, so a rebuild with no source change writes the same bytes.
//
// Style rules the source keeps, to hold to when editing it: roughness 0, solid
// fills, rounded rects {type: 3, value: 10}; Nunito (6) for entity and Chen
// names, Cascadia (3) for columns and types; crow's-foot arrowheads, which need
// Excalidraw 0.18 or later, on a dashed line for an optional end; one groupId
// per item so each inserts as a unit, except the connectors.
//
// Written compact, unlike entity-relationship.excalidrawlib: indented, the
// three files come to about 640 KB rather than 350 KB.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(__dirname, "..", "resources", "libraries");

const code = readFileSync(join(__dirname, "kanagawa-erd-library.js"), "utf8");
const context = {};
vm.createContext(context);
vm.runInContext(`${code}\n;this.KanagawaERD = KanagawaERD;`, context);
const K = context.KanagawaERD;

mkdirSync(OUT_DIR, { recursive: true });
for (const key of Object.keys(K.THEMES)) {
  const lib = { ...K.toLibrary(key), source: "https://github.com/rafael0rueda/excalidraw_desktop" };
  const file = join(OUT_DIR, `kanagawa-erd-${key}.excalidrawlib`);
  writeFileSync(file, JSON.stringify(lib) + "\n");
  console.log(`Wrote ${lib.libraryItems.length} library items to ${file}`);
}
