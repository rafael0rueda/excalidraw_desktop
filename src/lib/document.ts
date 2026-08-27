import { useCallback, useEffect, useRef, useState } from "react";
import { confirm, message, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import {
  loadSession,
  markCleanExit,
  pushRecent,
  readTextFile,
  saveSession,
  setWindowTitle,
  startupFile,
  writeTextFile,
} from "./api";
import { FILE_FILTER, basename, parseScene, sceneVersion, serializeScene } from "./scene";

/** Quiet period after the last edit before a snapshot is written. */
const AUTOSAVE_DELAY_MS = 1500;
/** Longest a snapshot may lag behind while the user keeps drawing. */
const AUTOSAVE_MAX_MS = 10_000;

/**
 * Stand-in scene version for a scene that matches nothing on disk. Real scene
 * versions are sums of element versions, so they are never negative.
 */
const NEVER_SAVED = -1;

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
  /** Records an orderly shutdown. Call immediately before destroying the window. */
  endSession: () => Promise<void>;
}

export function useDocument(api: ExcalidrawImperativeAPI | null) {
  const [path, setPath] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  // Scene version at the moment we last saved or loaded; anything else is dirty.
  const savedVersion = useRef(0);

  // The autosave timer fires outside React's render cycle, so it reads the
  // current document through refs rather than a captured closure.
  const pathRef = useRef(path);
  const dirtyRef = useRef(dirty);
  pathRef.current = path;
  dirtyRef.current = dirty;

  useEffect(() => {
    const name = path ? basename(path) : "Untitled";
    setWindowTitle(`${dirty ? "• " : ""}${name} — Excalidraw Desktop`).catch(() => {});
  }, [path, dirty]);

  const markSaved = useCallback(() => {
    if (api) savedVersion.current = sceneVersion(api.getSceneElements());
    setDirty(false);
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

  // ---------------------------------------------------------------- autosave

  // Snapshots are suppressed until startup has decided what to restore —
  // otherwise the empty initial canvas would overwrite the very snapshot we are
  // about to read back.
  const restored = useRef(false);
  const timer = useRef<number | null>(null);
  const pendingSince = useRef(0);
  // Identifies the state already on disk, so an idle app stops rewriting it.
  const lastSnapshot = useRef("");

  const snapshot = useCallback(async () => {
    if (!api || !restored.current) return;
    const key = `${sceneVersion(api.getSceneElements())}:${pathRef.current ?? ""}:${dirtyRef.current}`;
    if (key === lastSnapshot.current) return;
    try {
      await saveSession(pathRef.current, dirtyRef.current, serializeScene(api));
      lastSnapshot.current = key;
    } catch {
      // Autosave is best effort; a failure here must never interrupt drawing.
    }
  }, [api]);

  const scheduleSnapshot = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    const now = Date.now();
    if (pendingSince.current === 0) pendingSince.current = now;
    // Coalesce bursts of edits, but never let the deadline slide indefinitely.
    const wait = Math.max(0, Math.min(AUTOSAVE_DELAY_MS, pendingSince.current + AUTOSAVE_MAX_MS - now));
    timer.current = window.setTimeout(() => {
      timer.current = null;
      pendingSince.current = 0;
      void snapshot();
    }, wait);
  }, [snapshot]);

  const onSceneChange = useCallback(() => {
    if (!api) return;
    setDirty(sceneVersion(api.getSceneElements()) !== savedVersion.current);
    scheduleSnapshot();
  }, [api, scheduleSnapshot]);

  // Opening, saving and starting over are discrete events rather than bursts,
  // so record them straight away.
  useEffect(() => {
    void snapshot();
  }, [path, dirty, snapshot]);

  useEffect(() => () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
  }, []);

  const endSession = useCallback(async () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    await snapshot();
    await markCleanExit().catch(() => {});
  }, [snapshot]);

  // ------------------------------------------------------------ open / start

  const loadInto = useCallback(
    async (target: string, quiet = false) => {
      if (!api) return false;
      try {
        const scene = await parseScene(await readTextFile(target));
        api.updateScene({ elements: scene.elements, appState: scene.appState });
        if (scene.files) api.addFiles(Object.values(scene.files));
        api.scrollToContent(scene.elements, { fitToContent: true });
        await pushRecent(target);
        setPath(target);
        savedVersion.current = sceneVersion(scene.elements);
        setDirty(false);
        return true;
      } catch (err) {
        if (!quiet) await message(String(err), { title: "Could not open file", kind: "error" });
        return false;
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

  const restoreSession = useCallback(async () => {
    if (!api) return;
    const session = await loadSession().catch(() => null);
    if (!session) return;

    // Changes the file on disk does not have only survive an unclean exit —
    // after an orderly one the user already chose to save or discard them.
    if (!session.clean_exit && session.dirty) {
      const name = session.path ? basename(session.path) : "an unsaved drawing";
      const recover = await confirm(
        `${name} was left with unsaved changes when the app last closed. Restore them?`,
        { title: "Recover unsaved changes", kind: "warning", okLabel: "Restore", cancelLabel: "Discard" },
      );
      if (recover) {
        try {
          const scene = await parseScene(session.scene);
          // No scrollToContent here: the snapshot's own appState puts the user
          // back at the viewport they were working in.
          api.updateScene({ elements: scene.elements, appState: scene.appState });
          if (scene.files) api.addFiles(Object.values(scene.files));
          setPath(session.path);
          savedVersion.current = NEVER_SAVED;
          setDirty(true);
          return;
        } catch (err) {
          await message(String(err), { title: "Could not restore session", kind: "error" });
        }
      }
    }

    // Otherwise just reopen whatever was on screen, quietly — the file may well
    // have been moved or deleted since, and that is not worth a startup alert.
    if (session.path) await loadInto(session.path, true);
  }, [api, loadInto]);

  // Guarded rather than cancelled on cleanup: StrictMode runs effects twice in
  // development, and startup must not ask the user to recover twice.
  const startupDone = useRef(false);
  useEffect(() => {
    if (!api || startupDone.current) return;
    startupDone.current = true;
    void (async () => {
      try {
        // A path from the command line is an explicit request, so it outranks
        // whatever we happened to be doing last time.
        const file = await startupFile().catch(() => null);
        if (file && (await loadInto(file))) return;
        await restoreSession();
      } finally {
        restored.current = true;
      }
    })();
  }, [api, loadInto, restoreSession]);

  const state: DocumentState = { path, dirty };
  const actions: DocumentActions = {
    newDrawing,
    openDrawing,
    save,
    saveAs,
    onSceneChange,
    confirmDiscard,
    endSession,
  };
  return { state, actions };
}
