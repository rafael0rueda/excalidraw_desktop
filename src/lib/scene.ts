import { getSceneVersion, serializeAsJSON, loadFromBlob } from "@excalidraw/excalidraw";
import type { AppState, ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import type { TabView } from "./tabs";

export const FILE_EXTENSION = "excalidraw";
export const FILE_FILTER = { name: "Excalidraw drawing", extensions: [FILE_EXTENSION] };

/**
 * Excalidraw's scene version is a cheap running hash of element versions, which
 * is exactly what we need to know whether the document is dirty.
 */
export function sceneVersion(elements: readonly ExcalidrawElement[]): number {
  return getSceneVersion(elements);
}

export function serializeScene(api: ExcalidrawImperativeAPI): string {
  return serializeAsJSON(api.getSceneElements(), api.getAppState(), api.getFiles(), "local");
}

/**
 * A drawing with nothing in it, in the same format as everything else a tab
 * holds. `appState` carries the theme's defaults so a new tab starts painted.
 */
export function emptyScene(appState: Partial<AppState>): string {
  return serializeAsJSON([], appState, {}, "local");
}

/**
 * The viewport, which `.excalidraw` JSON deliberately does not carry: scroll and
 * zoom are flagged `export: false`, so a tab has to remember its own.
 */
export function currentView(api: ExcalidrawImperativeAPI): TabView {
  const { scrollX, scrollY, zoom } = api.getAppState();
  return { scrollX, scrollY, zoom: zoom.value };
}

/** Parses `.excalidraw` JSON text back into a scene Excalidraw can consume. */
export async function parseScene(text: string) {
  const blob = new Blob([text], { type: "application/json" });
  return loadFromBlob(blob, null, null);
}
