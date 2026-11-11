// Bundles page.jsx for WebKitGTK into iced/target/bench/webkit/.
import { build } from "esbuild";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, "../../target/bench/webkit");
mkdirSync(out, { recursive: true });
await build({
  entryPoints: [join(here, "page.jsx")],
  bundle: true,
  format: "esm",
  outdir: out,
  nodePaths: [join(here, "../../../node_modules")],
  define: { "process.env.NODE_ENV": '"production"', "process.env.IS_PREACT": '"false"' },
  minify: true,
  conditions: ["production"],
  logLevel: "error",
  absWorkingDir: join(here, "../../.."),
  loader: { ".woff2": "file" },
});
const assets = join(here, "../../../public/");
writeFileSync(join(out, "index.html"), `<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="page.css"><style>html,body{margin:0;height:100%;overflow:hidden}</style><div id="root"></div><script>window.EXCALIDRAW_ASSET_PATH = ${JSON.stringify("file://" + assets)};</script><script type="module" src="page.js"></script>`);
console.log("built", out);
