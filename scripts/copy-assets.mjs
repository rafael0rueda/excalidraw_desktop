// Copies Excalidraw's font assets into public/ so the app runs fully offline.
// Excalidraw otherwise fetches these from a CDN at runtime.
import { cp, mkdir, readdir, access } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "node_modules/@excalidraw/excalidraw/dist/prod/fonts");
const publicDir = join(root, "public");

try {
  await access(src);
} catch {
  console.error(`[assets] Excalidraw fonts not found at ${src} — run npm install first.`);
  process.exit(1);
}

await mkdir(publicDir, { recursive: true });

// Excalidraw resolves fonts both as `<assetPath>/fonts/<family>/<file>.woff2`
// and as flat `<assetPath>/<file>.woff2` depending on the code path, so mirror both.
await cp(src, join(publicDir, "fonts"), { recursive: true });

let flat = 0;
for (const family of await readdir(src, { withFileTypes: true })) {
  if (!family.isDirectory()) continue;
  for (const file of await readdir(join(src, family.name))) {
    if (!file.endsWith(".woff2")) continue;
    await cp(join(src, family.name, file), join(publicDir, file));
    flat++;
  }
}
console.log(`[assets] fonts copied (${flat} flat + fonts/ tree)`);
