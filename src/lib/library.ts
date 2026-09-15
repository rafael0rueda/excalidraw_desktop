import { useHandleLibrary } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI, LibraryItems } from "@excalidraw/excalidraw/types";
import { loadLibrary, saveLibrary } from "./api";
import erLibrary from "../../resources/libraries/entity-relationship.excalidrawlib?raw";

type LibraryAdapter = Extract<Parameters<typeof useHandleLibrary>[0], { adapter: unknown }>["adapter"];

/** The ER diagram shapes that `scripts/build-er-library.mjs` generates. */
function erShapes(): LibraryItems {
  return JSON.parse(erLibrary).libraryItems;
}

const adapter: LibraryAdapter = {
  async load() {
    const text = await loadLibrary();
    // Nothing saved yet, so a first launch opens with the ER shapes instead of
    // an empty sidebar. Once the library has been saved, with them or without,
    // this no longer applies. A file that exists but does not parse throws
    // here, which also stops Excalidraw saving over it.
    if (text === null) return { libraryItems: erShapes() };
    const data = JSON.parse(text);
    return { libraryItems: data.libraryItems ?? data.library ?? [] };
  },
  async save({ libraryItems }) {
    await saveLibrary(
      JSON.stringify({ type: "excalidrawlib", version: 2, source: "excalidraw-desktop", libraryItems }, null, 2),
    );
  },
};

/** Keeps Excalidraw's library in ~/.config/excalidraw-desktop/library.excalidrawlib. */
export function usePersistentLibrary(api: ExcalidrawImperativeAPI | null) {
  useHandleLibrary({ excalidrawAPI: api, adapter });
}

/** Adds the ER shapes to the library, skipping any already in it, and opens the sidebar. */
export function addErShapes(api: ExcalidrawImperativeAPI) {
  return api.updateLibrary({ libraryItems: erShapes(), merge: true, openLibraryMenu: true });
}
