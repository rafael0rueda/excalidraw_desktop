import { exportToBlob, exportToSvg } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import { blobToBase64, copyImageToClipboard, writeBinaryFile, writeTextFile } from "./api";

export interface ExportOptions {
  /** Export only the current selection rather than the whole scene. */
  selectionOnly?: boolean;
  transparent?: boolean;
  scale?: number;
}

/**
 * Expands a raw `selectedElementIds` filter with what Excalidraw's own
 * selection deliberately leaves out of it: a bound text label (selected via
 * its container, `containerId`, never by its own id) and the children of a
 * selected frame (`frameId`). Without this, exporting "selection" after
 * select-all silently drops labels and everything inside a selected frame.
 */
function expandSelection(all: readonly ExcalidrawElement[], selectedIds: Record<string, boolean>) {
  const included = new Set(all.filter((el) => selectedIds[el.id]).map((el) => el.id));
  for (const el of all) {
    if (el.frameId && included.has(el.frameId)) included.add(el.id);
  }
  // A second pass: a label bound to a container that a selected frame just
  // pulled in above is not itself selected or framed, only bound.
  for (const el of all) {
    if ("containerId" in el && el.containerId && included.has(el.containerId)) included.add(el.id);
  }
  return included;
}

function sceneFor(api: ExcalidrawImperativeAPI, opts: ExportOptions) {
  const all = api.getSceneElements();
  const appState = api.getAppState();
  const elements = opts.selectionOnly
    ? all.filter((el) => expandSelection(all, appState.selectedElementIds).has(el.id))
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
