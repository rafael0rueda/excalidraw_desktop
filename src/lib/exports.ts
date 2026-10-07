import { exportToBlob, exportToSvg } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { copyImageToClipboard, writeBinaryFile, writeTextFile } from "./api";
import { expandSelection } from "./selection";

export interface ExportOptions {
  /** Export only the current selection rather than the whole scene. */
  selectionOnly?: boolean;
  transparent?: boolean;
  scale?: number;
  /** Store the drawing inside the image, so opening it in Excalidraw brings the scene back. */
  embedScene?: boolean;
}

function sceneFor(api: ExcalidrawImperativeAPI, opts: ExportOptions) {
  const all = api.getSceneElements();
  const appState = api.getAppState();
  // Worked out once, not per element: inside the filter it made a selection
  // export quadratic in the size of the drawing.
  const included = opts.selectionOnly ? expandSelection(all, appState.selectedElementIds) : null;
  const elements = included ? all.filter((el) => included.has(el.id)) : all;
  return { elements, appState, files: api.getFiles() };
}

export async function toPngBlob(api: ExcalidrawImperativeAPI, opts: ExportOptions = {}) {
  const { elements, appState, files } = sceneFor(api, opts);
  if (!elements.length) throw new Error("Nothing to export.");
  const scale = opts.scale ?? 2;
  return exportToBlob({
    elements,
    files,
    appState: {
      ...appState,
      exportBackground: !opts.transparent,
      exportScale: scale,
      exportEmbedScene: !!opts.embedScene,
    },
    // What actually sizes the image. `exportToBlob` reads `exportScale` only
    // when `maxWidthOrHeight` is given; without this every export came out at
    // 1x whatever the menu said.
    getDimensions: (width: number, height: number) => ({ width: width * scale, height: height * scale, scale }),
    mimeType: "image/png",
    quality: 1,
  });
}

async function pngBytes(api: ExcalidrawImperativeAPI, opts: ExportOptions) {
  return new Uint8Array(await (await toPngBlob(api, opts)).arrayBuffer());
}

export async function savePng(api: ExcalidrawImperativeAPI, path: string, opts: ExportOptions = {}) {
  await writeBinaryFile(path, await pngBytes(api, opts));
}

export async function saveSvg(api: ExcalidrawImperativeAPI, path: string, opts: ExportOptions = {}) {
  const { elements, appState, files } = sceneFor(api, opts);
  if (!elements.length) throw new Error("Nothing to export.");
  // No scale: an SVG is drawn at whatever size it is shown.
  const svg = await exportToSvg({
    elements,
    files,
    appState: {
      ...appState,
      exportBackground: !opts.transparent,
      exportScale: 1,
      exportEmbedScene: !!opts.embedScene,
    },
  });
  await writeTextFile(path, new XMLSerializer().serializeToString(svg));
}

/**
 * Routed through the native GTK clipboard rather than navigator.clipboard —
 * WebKitGTK gates the async clipboard API behind user activation that an
 * app-driven copy does not always carry. Never embeds the scene: an image
 * pasted elsewhere has no use for it.
 */
export async function copyPngToClipboard(api: ExcalidrawImperativeAPI, opts: ExportOptions = {}) {
  await copyImageToClipboard(await pngBytes(api, { ...opts, embedScene: false }));
}
