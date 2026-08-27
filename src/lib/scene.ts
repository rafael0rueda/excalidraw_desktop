import { getSceneVersion, serializeAsJSON, loadFromBlob } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";

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

/** Parses `.excalidraw` JSON text back into a scene Excalidraw can consume. */
export async function parseScene(text: string) {
  const blob = new Blob([text], { type: "application/json" });
  return loadFromBlob(blob, null, null);
}

export function basename(path: string): string {
  return path.split("/").pop() || path;
}
