import { message } from "@tauri-apps/plugin-dialog";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { pickSavePath } from "./api";
import { copyPngToClipboard, savePng, saveSvg, type ExportOptions } from "./exports";

/** Beside the drawing when it has a file, so an export lands next to its source. */
function suggestedPath(path: string | null, ext: string) {
  return path ? `${path.replace(/\.excalidraw$/, "")}.${ext}` : `Untitled.${ext}`;
}

async function report(err: unknown) {
  await message(String(err instanceof Error ? err.message : err), {
    title: "Export failed",
    kind: "error",
  });
}

export async function exportPng(
  api: ExcalidrawImperativeAPI,
  path: string | null,
  opts: ExportOptions = {},
) {
  try {
    const target = await pickSavePath("png", suggestedPath(path, "png"));
    if (target) await savePng(api, target, opts);
  } catch (err) {
    await report(err);
  }
}

export async function exportSvg(
  api: ExcalidrawImperativeAPI,
  path: string | null,
  opts: ExportOptions = {},
) {
  try {
    const target = await pickSavePath("svg", suggestedPath(path, "svg"));
    if (target) await saveSvg(api, target, opts);
  } catch (err) {
    await report(err);
  }
}

export async function copyToClipboard(api: ExcalidrawImperativeAPI, opts: ExportOptions = {}) {
  try {
    await copyPngToClipboard(api, opts);
  } catch (err) {
    await report(err);
  }
}
