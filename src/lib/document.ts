import { useCallback, useEffect, useRef, useState } from "react";
import { confirm, message, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { pushRecent, readTextFile, setWindowTitle, startupFile, writeTextFile } from "./api";
import { FILE_FILTER, basename, parseScene, sceneVersion, serializeScene } from "./scene";

export interface DocumentState {
  path: string | null;
  dirty: boolean;
}

export interface DocumentActions {
  newDrawing: () => Promise<void>;
  openDrawing: (path?: string) => Promise<void>;
  save: () => Promise<boolean>;
  saveAs: () => Promise<boolean>;
  onSceneChange: () => void;
  /** True if it is safe to throw away the current drawing. */
  confirmDiscard: () => Promise<boolean>;
}

export function useDocument(api: ExcalidrawImperativeAPI | null) {
  const [path, setPath] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  // Scene version at the moment we last saved or loaded; anything else is dirty.
  const savedVersion = useRef(0);

  useEffect(() => {
    const name = path ? basename(path) : "Untitled";
    setWindowTitle(`${dirty ? "• " : ""}${name} — Excalidraw Desktop`).catch(() => {});
  }, [path, dirty]);

  const markSaved = useCallback(() => {
    if (api) savedVersion.current = sceneVersion(api.getSceneElements());
    setDirty(false);
  }, [api]);

  const onSceneChange = useCallback(() => {
    if (!api) return;
    setDirty(sceneVersion(api.getSceneElements()) !== savedVersion.current);
  }, [api]);

  const writeTo = useCallback(
    async (target: string) => {
      if (!api) return false;
      try {
        await writeTextFile(target, serializeScene(api));
        await pushRecent(target);
        setPath(target);
        markSaved();
        return true;
      } catch (err) {
        await message(String(err), { title: "Could not save", kind: "error" });
        return false;
      }
    },
    [api, markSaved],
  );

  const saveAs = useCallback(async () => {
    const target = await saveDialog({
      title: "Save drawing as",
      defaultPath: path ?? "Untitled.excalidraw",
      filters: [FILE_FILTER],
    });
    if (!target) return false;
    return writeTo(target.endsWith(".excalidraw") ? target : `${target}.excalidraw`);
  }, [path, writeTo]);

  const save = useCallback(async () => (path ? writeTo(path) : saveAs()), [path, saveAs, writeTo]);

  const confirmDiscard = useCallback(async () => {
    if (!dirty) return true;
    const name = path ? basename(path) : "this drawing";
    const keep = await confirm(`${name} has unsaved changes. Save before continuing?`, {
      title: "Unsaved changes",
      kind: "warning",
      okLabel: "Save",
      cancelLabel: "Discard",
    });
    return keep ? await save() : true;
  }, [dirty, path, save]);

  const loadInto = useCallback(
    async (target: string) => {
      if (!api) return;
      try {
        const scene = await parseScene(await readTextFile(target));
        api.updateScene({ elements: scene.elements, appState: scene.appState });
        if (scene.files) api.addFiles(Object.values(scene.files));
        api.scrollToContent(scene.elements, { fitToContent: true });
        await pushRecent(target);
        setPath(target);
        savedVersion.current = sceneVersion(scene.elements);
        setDirty(false);
      } catch (err) {
        await message(String(err), { title: "Could not open file", kind: "error" });
      }
    },
    [api],
  );

  const openDrawing = useCallback(
    async (target?: string) => {
      if (!(await confirmDiscard())) return;
      let chosen = target;
      if (!chosen) {
        const picked = await openDialog({
          title: "Open drawing",
          multiple: false,
          filters: [FILE_FILTER],
        });
        if (typeof picked !== "string") return;
        chosen = picked;
      }
      await loadInto(chosen);
    },
    [confirmDiscard, loadInto],
  );

  const newDrawing = useCallback(async () => {
    if (!api) return;
    if (!(await confirmDiscard())) return;
    api.resetScene();
    setPath(null);
    savedVersion.current = sceneVersion(api.getSceneElements());
    setDirty(false);
  }, [api, confirmDiscard]);

  // Honour a path handed to us on the command line (file-manager double click).
  useEffect(() => {
    if (!api) return;
    let cancelled = false;
    startupFile()
      .then((file) => {
        if (file && !cancelled) loadInto(file);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [api, loadInto]);

  const state: DocumentState = { path, dirty };
  const actions: DocumentActions = { newDrawing, openDrawing, save, saveAs, onSceneChange, confirmDiscard };
  return { state, actions };
}
