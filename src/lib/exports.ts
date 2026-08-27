import { exportToBlob, exportToSvg } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { blobToBase64, copyImageToClipboard, writeBinaryFile, writeTextFile } from "./api";

export interface ExportOptions {
  /** Export only the current selection rather than the whole scene. */
  selectionOnly?: boolean;
  transparent?: boolean;
  scale?: number;
}

function sceneFor(api: ExcalidrawImperativeAPI, opts: ExportOptions) {
  const all = api.getSceneElements();
  const appState = api.getAppState();
  const elements = opts.selectionOnly
    ? all.filter((el) => appState.selectedElementIds[el.id])
    : all;
  return { elements, appState, files: api.getFiles() };
}

export async function toPngBlob(api: ExcalidrawImperativeAPI, opts: ExportOptions = {}) {
  const { elements, appState, files } = sceneFor(api, opts);
  if (!elements.length) throw new Error("Nothing to export.");
  return exportToBlob({
    elements,
    files,
    appState: {
      ...appState,
      exportBackground: !opts.transparent,
      exportScale: opts.scale ?? 2,
    },
    mimeType: "image/png",
    quality: 1,
  });
}

export async function savePng(api: ExcalidrawImperativeAPI, path: string, opts: ExportOptions = {}) {
  const blob = await toPngBlob(api, opts);
  await writeBinaryFile(path, await blobToBase64(blob));
}

export async function saveSvg(api: ExcalidrawImperativeAPI, path: string, opts: ExportOptions = {}) {
  const { elements, appState, files } = sceneFor(api, opts);
  if (!elements.length) throw new Error("Nothing to export.");
  const svg = await exportToSvg({
    elements,
    files,
    appState: { ...appState, exportBackground: !opts.transparent, exportScale: opts.scale ?? 1 },
  });
  await writeTextFile(path, new XMLSerializer().serializeToString(svg));
}

/**
 * Routed through the native GTK clipboard rather than navigator.clipboard —
 * WebKitGTK gates the async clipboard API behind user activation that an
 * app-driven copy does not always carry.
 */
export async function copyPngToClipboard(api: ExcalidrawImperativeAPI, opts: ExportOptions = {}) {
  const blob = await toPngBlob(api, opts);
  await copyImageToClipboard(await blobToBase64(blob));
}
