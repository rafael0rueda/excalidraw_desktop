import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { confirm, message, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { CaptureUpdateAction } from "@excalidraw/excalidraw";
import type { AppState, ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import {
  loadSession,
  markCleanExit,
  pushRecent,
  readTextFile,
  saveSession,
  setWindowTitle,
  startupFile,
  writeTextFile,
  type TabSnapshot,
} from "./api";
import {
  FILE_FILTER,
  currentView,
  emptyScene,
  parseScene,
  sceneVersion,
  serializeScene,
} from "./scene";
import {
  NEVER_SAVED,
  UNPARSED,
  basename,
  findByPath,
  newTabId,
  relativeId,
  successorId,
  tabTitle,
  type TabContent,
  type TabMeta,
  type TabView,
} from "./tabs";

/** Quiet period after the last edit before a snapshot is written. */
const AUTOSAVE_DELAY_MS = 1500;
/** Longest a snapshot may lag behind while the user keeps drawing. */
const AUTOSAVE_MAX_MS = 10_000;

type ParsedScene = Awaited<ReturnType<typeof parseScene>>;

export interface DocumentState {
  tabs: TabMeta[];
  activeId: string;
  /** The active tab's path and dirty flag, hoisted for the title and the menu. */
  path: string | null;
  dirty: boolean;
}

/**
 * The `appState` fields a theme owns, merged into every scene replacement.
 *
 * This cannot be done afterwards: Excalidraw commits a replaced scene on its
 * own render pass, which lands after ours, so a repaint scheduled from an
 * effect gets overwritten. Note that `currentItemStrokeColor` is never in a
 * `.excalidraw` file (it is flagged `export: false`) — `loadFromBlob` fills it
 * with Excalidraw's own `#1e1e1e`, which is invisible on a dark canvas.
 */
export interface ThemedDefaults {
  viewBackgroundColor: string;
  currentItemStrokeColor: string;
  currentItemBackgroundColor: string;
}

export interface DocumentActions {
  newTab: () => Promise<void>;
  openDrawing: (path?: string) => Promise<void>;
  closeTab: (id?: string) => Promise<void>;
  selectTab: (id: string) => Promise<void>;
  /** Steps `delta` tabs from the active one, wrapping at both ends. */
  selectRelative: (delta: number) => Promise<void>;
  save: () => Promise<boolean>;
  saveAs: () => Promise<boolean>;
  onSceneChange: () => void;
  /** True if it is safe to close the window: every dirty tab has been dealt with. */
  confirmDiscard: () => Promise<boolean>;
  /** Records an orderly shutdown. Call immediately before destroying the window. */
  endSession: () => Promise<void>;
}

/**
 * Every open drawing, of which exactly one is on screen.
 *
 * Excalidraw is a single editor instance, so a tab that is not active lives as
 * `.excalidraw` text in `store` — the same format we save, open and snapshot.
 * Switching tabs is therefore a scene load, which is a path that already works,
 * rather than an attempt to keep several live scenes in the air at once.
 */
export function useDocument(api: ExcalidrawImperativeAPI | null, themed: ThemedDefaults) {
  const initial = useMemo<TabMeta>(() => ({ id: newTabId(), path: null, dirty: false }), []);
  const [tabs, setTabs] = useState<TabMeta[]>([initial]);
  const [activeId, setActiveId] = useState(initial.id);

  // Read through a ref so a new theme does not churn every callback's identity.
  const themedRef = useRef(themed);
  themedRef.current = themed;

  // Callbacks that run outside the render cycle — the autosave timer, anything
  // resumed after an await — read the current tabs through refs. The active id
  // is also written eagerly, before React re-renders, so that work started
  // during a switch files its results under the right tab.
  const tabsRef = useRef(tabs);
  const activeRef = useRef(activeId);
  tabsRef.current = tabs;
  activeRef.current = activeId;

  /** The heavy half of each tab, kept out of React state so edits do not re-render. */
  const store = useRef(new Map<string, TabContent>());
  /** Scene version at the moment the active tab was last saved or loaded. */
  const savedVersion = useRef(0);

  /**
   * False between handing Excalidraw a scene and Excalidraw committing it.
   * `getSceneElements()` still reports the outgoing drawing in that window, so
   * capturing then would file one tab's scene under another tab's id.
   */
  const committed = useRef(true);

  const emptyContent = useCallback(
    (): TabContent => ({
      scene: emptyScene(themedRef.current),
      view: null,
      savedVersion: 0,
      rev: 1,
    }),
    [],
  );

  const contentOf = useCallback(
    (id: string): TabContent => {
      let content = store.current.get(id);
      if (!content) {
        content = emptyContent();
        store.current.set(id, content);
      }
      return content;
    },
    [emptyContent],
  );

  // -------------------------------------------------------------- the scene

  /** Files what is on screen under the active tab, so the tab can be left. */
  const capture = useCallback(() => {
    if (!api || !committed.current) return;
    const id = activeRef.current;
    const scene = serializeScene(api);
    const prev = store.current.get(id);
    store.current.set(id, {
      scene,
      view: currentView(api),
      savedVersion: savedVersion.current,
      // The revision only moves when the drawing does, which is what lets
      // autosave leave untouched tabs alone.
      rev: prev && prev.scene === scene ? prev.rev : (prev?.rev ?? 0) + 1,
    });
  }, [api]);

  /** Puts an already-parsed scene on screen and records it as tab `id`. */
  const applyScene = useCallback(
    (
      id: string,
      text: string,
      scene: ParsedScene,
      opts: { view?: TabView | null; fit?: boolean; savedVersion?: number } = {},
    ) => {
      if (!api) return;
      const view = opts.view
        ? {
            scrollX: opts.view.scrollX,
            scrollY: opts.view.scrollY,
            zoom: { value: opts.view.zoom } as AppState["zoom"],
          }
        : {};
      committed.current = false;
      api.updateScene({
        elements: scene.elements,
        appState: { ...scene.appState, ...view, ...themedRef.current },
        // Scene initialisation, not an edit: putting it on the undo stack would
        // let Ctrl+Z rewind past the drawing that is now on screen.
        captureUpdate: CaptureUpdateAction.NEVER,
      });
      if (scene.files) api.addFiles(Object.values(scene.files));
      if (opts.fit) api.scrollToContent(scene.elements, { fitToContent: true });
      // Undo history belongs to the Excalidraw instance rather than to any one
      // drawing, so without this Ctrl+Z would undo an edit made in another tab.
      api.history.clear();

      savedVersion.current = opts.savedVersion ?? sceneVersion(scene.elements);
      const prev = store.current.get(id);
      store.current.set(id, {
        scene: text,
        view: opts.view ?? null,
        savedVersion: savedVersion.current,
        rev: prev && prev.scene === text ? prev.rev : (prev?.rev ?? 0) + 1,
      });
    },
    [api],
  );

  /** Brings tab `id` on screen from its stored text. */
  const show = useCallback(
    async (id: string) => {
      if (!api) return;
      const content = contentOf(id);
      let scene: ParsedScene;
      try {
        scene = await parseScene(content.scene);
      } catch (err) {
        // Only reachable if a snapshot on disk is corrupt; an empty canvas is a
        // better answer than a tab that cannot be opened at all.
        await message(String(err), { title: "Could not open tab", kind: "error" });
        scene = await parseScene(emptyScene(themedRef.current));
      }
      applyScene(id, content.scene, scene, {
        view: content.view,
        // A tab restored from a session has not been parsed yet, so its saved
        // version is worked out here rather than at startup.
        savedVersion: content.savedVersion === UNPARSED ? undefined : content.savedVersion,
      });
    },
    [api, applyScene, contentOf],
  );

  /** Replaces the whole tab set, dropping the content of tabs that have gone. */
  const replaceTabs = useCallback((next: TabMeta[], active: string) => {
    for (const id of [...store.current.keys()]) {
      if (!next.some((tab) => tab.id === id)) store.current.delete(id);
    }
    tabsRef.current = next;
    activeRef.current = active;
    setTabs(next);
    setActiveId(active);
  }, []);

  // --------------------------------------------------------------- switching

  const selectTab = useCallback(
    async (id: string) => {
      if (id === activeRef.current || !tabsRef.current.some((tab) => tab.id === id)) return;
      capture();
      activeRef.current = id;
      setActiveId(id);
      await show(id);
    },
    [capture, show],
  );

  const selectRelative = useCallback(
    async (delta: number) => {
      const id = relativeId(tabsRef.current, activeRef.current, delta);
      if (id) await selectTab(id);
    },
    [selectTab],
  );

  const newTab = useCallback(async () => {
    capture();
    const tab: TabMeta = { id: newTabId(), path: null, dirty: false };
    store.current.set(tab.id, emptyContent());
    const next = [...tabsRef.current, tab];
    tabsRef.current = next;
    activeRef.current = tab.id;
    setTabs(next);
    setActiveId(tab.id);
    await show(tab.id);
  }, [capture, emptyContent, show]);

  // ------------------------------------------------------------------ saving

  const writeTo = useCallback(
    async (id: string, target: string) => {
      capture();
      const content = store.current.get(id);
      if (!content) return false;
      try {
        await writeTextFile(target, content.scene);
        await pushRecent(target);
      } catch (err) {
        await message(String(err), { title: "Could not save", kind: "error" });
        return false;
      }
      if (id === activeRef.current) {
        // `committed` false means nothing has been drawn since this tab came on
        // screen, so the version recorded then still describes what we wrote.
        if (committed.current && api) savedVersion.current = sceneVersion(api.getSceneElements());
        store.current.set(id, { ...content, savedVersion: savedVersion.current });
      } else {
        // Worked out the next time the tab is shown, which is the only moment
        // its elements exist as anything but text.
        store.current.set(id, { ...content, savedVersion: UNPARSED });
      }
      setTabs((prev) =>
        prev.map((tab) => (tab.id === id ? { ...tab, path: target, dirty: false } : tab)),
      );
      return true;
    },
    [api, capture],
  );

  const saveTabAs = useCallback(
    async (id: string) => {
      const tab = tabsRef.current.find((t) => t.id === id);
      if (!tab) return false;
      const target = await saveDialog({
        title: "Save drawing as",
        defaultPath: tab.path ?? "Untitled.excalidraw",
        filters: [FILE_FILTER],
      });
      if (!target) return false;
      return writeTo(id, target.endsWith(".excalidraw") ? target : `${target}.excalidraw`);
    },
    [writeTo],
  );

  const saveTab = useCallback(
    async (id: string) => {
      const tab = tabsRef.current.find((t) => t.id === id);
      if (!tab) return false;
      return tab.path ? writeTo(id, tab.path) : saveTabAs(id);
    },
    [saveTabAs, writeTo],
  );

  const save = useCallback(() => saveTab(activeRef.current), [saveTab]);
  const saveAs = useCallback(() => saveTabAs(activeRef.current), [saveTabAs]);

  // ---------------------------------------------------------------- closing

  /** True once tab `id` may be thrown away. */
  const confirmTab = useCallback(
    async (id: string) => {
      const tab = tabsRef.current.find((t) => t.id === id);
      if (!tab || !tab.dirty) return true;
      const keep = await confirm(`${tabTitle(tab)} has unsaved changes. Save before continuing?`, {
        title: "Unsaved changes",
        kind: "warning",
        okLabel: "Save",
        cancelLabel: "Discard",
      });
      return keep ? await saveTab(id) : true;
    },
    [saveTab],
  );

  const closeTab = useCallback(
    async (id?: string) => {
      const target = id ?? activeRef.current;
      if (!tabsRef.current.some((tab) => tab.id === target)) return;
      // Show a drawing before asking about it: a prompt naming a tab the user
      // cannot see is a prompt they cannot answer.
      if (target !== activeRef.current) await selectTab(target);
      if (!(await confirmTab(target))) return;

      const next = successorId(tabsRef.current, target);
      const remaining = tabsRef.current.filter((tab) => tab.id !== target);
      if (!remaining.length || !next) {
        // Closing the last tab empties the canvas rather than quitting: closing
        // a drawing and closing the app are different requests.
        const fresh: TabMeta = { id: newTabId(), path: null, dirty: false };
        store.current.set(fresh.id, emptyContent());
        replaceTabs([fresh], fresh.id);
        await show(fresh.id);
        return;
      }
      replaceTabs(remaining, next);
      await show(next);
    },
    [confirmTab, emptyContent, replaceTabs, selectTab, show],
  );

  const confirmDiscard = useCallback(async () => {
    for (const tab of [...tabsRef.current]) {
      if (!tab.dirty) continue;
      if (tab.id !== activeRef.current) await selectTab(tab.id);
      if (!(await confirmTab(tab.id))) return false;
    }
    return true;
  }, [confirmTab, selectTab]);

  // ---------------------------------------------------------------- autosave

  // Snapshots are suppressed until startup has decided what to restore —
  // otherwise the empty initial canvas would overwrite the very snapshots we
  // are about to read back.
  const restored = useRef(false);
  const timer = useRef<number | null>(null);
  const pendingSince = useRef(0);
  /** Revision of each tab as the session file last saw it. */
  const written = useRef(new Map<string, number>());

  const snapshot = useCallback(async () => {
    if (!api || !restored.current) return;
    capture();
    const payload: TabSnapshot[] = tabsRef.current.map((tab) => {
      const content = store.current.get(tab.id);
      const snap: TabSnapshot = { id: tab.id, path: tab.path, dirty: tab.dirty };
      // A drawing nobody has touched since the last snapshot is already on disk;
      // sending it again would rewrite every open tab on every keystroke.
      if (content && written.current.get(tab.id) !== content.rev) snap.scene = content.scene;
      return snap;
    });
    try {
      await saveSession(payload, activeRef.current);
      for (const snap of payload) {
        const rev = store.current.get(snap.id)?.rev;
        if (snap.scene !== undefined && rev !== undefined) written.current.set(snap.id, rev);
      }
      for (const id of [...written.current.keys()]) {
        if (!tabsRef.current.some((tab) => tab.id === id)) written.current.delete(id);
      }
    } catch {
      // Autosave is best effort; a failure here must never interrupt drawing.
    }
  }, [api, capture]);

  const scheduleSnapshot = useCallback(() => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    const now = Date.now();
    if (pendingSince.current === 0) pendingSince.current = now;
    // Coalesce bursts of edits, but never let the deadline slide indefinitely.
    const wait = Math.max(
      0,
      Math.min(AUTOSAVE_DELAY_MS, pendingSince.current + AUTOSAVE_MAX_MS - now),
    );
    timer.current = window.setTimeout(() => {
      timer.current = null;
      pendingSince.current = 0;
      void snapshot();
    }, wait);
  }, [snapshot]);

  const onSceneChange = useCallback(() => {
    if (!api) return;
    // Excalidraw has committed whatever we last handed it, so the scene on
    // screen is once again the active tab's.
    committed.current = true;
    const dirty = sceneVersion(api.getSceneElements()) !== savedVersion.current;
    setTabs((prev) =>
      prev.some((tab) => tab.id === activeRef.current && tab.dirty !== dirty)
        ? prev.map((tab) => (tab.id === activeRef.current ? { ...tab, dirty } : tab))
        : prev,
    );
    scheduleSnapshot();
  }, [api, scheduleSnapshot]);

  // Opening, saving, switching and closing are discrete events rather than
  // bursts, so record them straight away.
  useEffect(() => {
    void snapshot();
  }, [tabs, activeId, snapshot]);

  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    },
    [],
  );

  const endSession = useCallback(async () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    await snapshot();
    await markCleanExit().catch(() => {});
  }, [snapshot]);

  // ------------------------------------------------------------ open / start

  /** The active tab is worth reusing when it holds nothing the user would miss. */
  const pristineActive = useCallback(() => {
    const tab = tabsRef.current.find((t) => t.id === activeRef.current);
    if (!tab || tab.path || tab.dirty) return null;
    if (api && committed.current && api.getSceneElements().length) return null;
    return tab.id;
  }, [api]);

  const openDrawing = useCallback(
    async (target?: string) => {
      if (!api) return;
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

      // Already open: go to it rather than making a second copy of the same
      // file, which could then be saved over itself from two directions.
      const existing = findByPath(tabsRef.current, chosen);
      if (existing) {
        await selectTab(existing.id);
        return;
      }

      let text: string;
      let scene: ParsedScene;
      try {
        text = await readTextFile(chosen);
        scene = await parseScene(text);
      } catch (err) {
        await message(String(err), { title: "Could not open file", kind: "error" });
        return;
      }

      capture();
      // Reuse an untouched tab rather than leaving an empty one behind.
      const file = chosen;
      const reuse = pristineActive();
      const id = reuse ?? newTabId();
      const opened: TabMeta = { id, path: file, dirty: false };
      const next = reuse
        ? tabsRef.current.map((tab) => (tab.id === id ? opened : tab))
        : [...tabsRef.current, opened];
      tabsRef.current = next;
      activeRef.current = id;
      setTabs(next);
      setActiveId(id);
      applyScene(id, text, scene, { fit: true });
      await pushRecent(file).catch(() => {});
    },
    [api, applyScene, capture, pristineActive, selectTab],
  );

  const restoreSession = useCallback(async () => {
    if (!api) return;
    const session = await loadSession().catch(() => null);
    if (!session || !session.tabs.length) return;

    // Changes the files on disk do not have only survive an unclean exit —
    // after an orderly one the user already chose to save or discard them.
    const unsaved = session.tabs.filter((tab) => tab.dirty);
    if (!session.clean_exit && unsaved.length) {
      const subject =
        unsaved.length === 1
          ? `${unsaved[0].path ? basename(unsaved[0].path) : "An unsaved drawing"} was`
          : `${unsaved.length} drawings were`;
      const recover = await confirm(
        `${subject} left with unsaved changes when the app last closed. Restore them?`,
        {
          title: "Recover unsaved changes",
          kind: "warning",
          okLabel: "Restore",
          cancelLabel: "Discard",
        },
      );
      if (recover) {
        for (const tab of session.tabs) {
          store.current.set(tab.id, {
            scene: tab.scene,
            // No stored view: the snapshot's own appState puts the user back at
            // the viewport they were working in.
            view: null,
            // A recovered drawing matches nothing on disk, so it stays dirty
            // until the user actually saves it.
            savedVersion: tab.dirty ? NEVER_SAVED : UNPARSED,
            rev: 1,
          });
          written.current.set(tab.id, 1);
        }
        const metas = session.tabs.map(({ id, path, dirty }) => ({ id, path, dirty }));
        const active = metas.some((t) => t.id === session.active) ? session.active! : metas[0].id;
        replaceTabs(metas, active);
        await show(active);
        return;
      }
    }

    // Otherwise just reopen whatever was on screen, quietly — a file may well
    // have been moved or deleted since, and that is not worth a startup alert.
    // Reopening deliberately does not touch the recent list: these files were
    // added to it when they were opened, and rewriting it on every launch would
    // order it by tab rather than by when the user last reached for something.
    const opened: TabMeta[] = [];
    for (const tab of session.tabs) {
      if (!tab.path) continue;
      const text = await readTextFile(tab.path).catch(() => null);
      if (text === null) continue;
      opened.push({ id: tab.id, path: tab.path, dirty: false });
      store.current.set(tab.id, { scene: text, view: null, savedVersion: UNPARSED, rev: 1 });
    }
    if (!opened.length) return;
    const active = opened.some((t) => t.id === session.active) ? session.active! : opened[0].id;
    replaceTabs(opened, active);
    await show(active);
  }, [api, replaceTabs, show]);

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
        if (file) {
          await openDrawing(file);
          return;
        }
        await restoreSession();
      } finally {
        restored.current = true;
      }
    })();
  }, [api, openDrawing, restoreSession]);

  // ----------------------------------------------------------------- window

  const active = tabs.find((tab) => tab.id === activeId) ?? tabs[0];
  useEffect(() => {
    const name = active ? tabTitle(active) : "Untitled";
    setWindowTitle(`${active?.dirty ? "• " : ""}${name} — Excalidraw Desktop`).catch(() => {});
  }, [active]);

  const state: DocumentState = {
    tabs,
    activeId,
    path: active?.path ?? null,
    dirty: active?.dirty ?? false,
  };
  const actions: DocumentActions = {
    newTab,
    openDrawing,
    closeTab,
    selectTab,
    selectRelative,
    save,
    saveAs,
    onSceneChange,
    confirmDiscard,
    endSession,
  };
  return { state, actions };
}
