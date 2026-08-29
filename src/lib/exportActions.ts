import { message, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { copyPngToClipboard, savePng, saveSvg, type ExportOptions } from "./exports";
import { basename } from "./tabs";

function suggestedName(path: string | null, ext: string) {
  const stem = path ? basename(path).replace(/\.excalidraw$/, "") : "Untitled";
  return `${stem}.${ext}`;
}

async function pick(path: string | null, ext: string, label: string) {
  return saveDialog({
    title: `Export as ${label}`,
    defaultPath: suggestedName(path, ext),
    filters: [{ name: label, extensions: [ext] }],
  });
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
  const target = await pick(path, "png", "PNG image");
  if (!target) return;
  try {
    await savePng(api, target.endsWith(".png") ? target : `${target}.png`, opts);
  } catch (err) {
    await report(err);
  }
}

export async function exportSvg(
  api: ExcalidrawImperativeAPI,
  path: string | null,
  opts: ExportOptions = {},
) {
  const target = await pick(path, "svg", "SVG image");
  if (!target) return;
  try {
    await saveSvg(api, target.endsWith(".svg") ? target : `${target}.svg`, opts);
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
