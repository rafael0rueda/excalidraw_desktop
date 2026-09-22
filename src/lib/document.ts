import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { message } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { CaptureUpdateAction } from "@excalidraw/excalidraw";
import type { AppState, ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import {
  keepUnreadableSnapshot,
  loadSession,
  markCleanExit,
  pickOpenPath,
  pickSavePath,
  pushRecent,
  readTextFile,
  saveSession,
  setWindowTitle,
  startupFiles,
  OPEN_FILES_EVENT,
  writeTextFile,
  type TabSnapshot,
} from "./api";
import {
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
/**
 * Consecutive failed snapshots before the user is told. A single failure is
 * not worth a dialog — the next one a second later usually works — but a
 * standing one means crash recovery is dead, and nothing else would say so.
 */
const AUTOSAVE_FAILURES_BEFORE_WARNING = 3;

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
   * False between deciding to put a scene on screen and Excalidraw committing
   * it. `getSceneElements()` still reports the outgoing drawing for that whole
   * window — which starts at `show()`, before the scene has even been parsed —
   * so capturing then would file one tab's scene under another tab's id.
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

  /**
   * The active tab, scene version (deleted elements included, so it only ever
   * grows), background and file count at the last capture. Serialising embeds
   * every image in the drawing, so autosave skips it while this stands still.
   */
  const capturedMark = useRef("");

  /**
   * Files what is on screen under the active tab, so the tab can be left.
   * `onlyIfChanged` is for autosave alone: some appState edits (the grid, say)
   * do not move the mark, and saving or switching tabs must never miss one.
   */
  const capture = useCallback((opts: { onlyIfChanged?: boolean } = {}) => {
    if (!api || !committed.current) return;
    const id = activeRef.current;
    const prev = store.current.get(id);
    const all = api.getSceneElementsIncludingDeleted();
    const mark = [
      id,
      sceneVersion(all),
      all.length,
      api.getAppState().viewBackgroundColor,
      Object.keys(api.getFiles()).length,
    ].join(":");
    if (opts.onlyIfChanged && prev && mark === capturedMark.current) return;
    capturedMark.current = mark;
    const scene = serializeScene(api);
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

  /** A new untitled tab with its empty content in place; not yet in the tab set. */
  const blankTab = useCallback((): TabMeta => {
    const tab: TabMeta = { id: newTabId(), path: null, dirty: false };
    store.current.set(tab.id, emptyContent());
    return tab;
  }, [emptyContent]);

  /** Brings tab `id` on screen from its stored text. */
  const show = useCallback(
    async (id: string): Promise<void> => {
      if (!api) return;
      const content = contentOf(id);
      // Lowered here rather than in `applyScene`, which is only reached after
      // the parse below has awaited. In that window `activeRef` already names
      // the incoming tab while the canvas still holds the outgoing one, so a
      // capture landing there filed one drawing under another tab's id. The
      // caller has already captured what it is leaving, so suppressing
      // captures from here until the new scene is committed loses nothing.
      committed.current = false;
      let scene: ParsedScene;
      try {
        scene = await parseScene(content.scene);
      } catch (err) {
        // Not shown as an empty canvas: the tab would keep its path, and the
        // next Save would write that empty canvas over the user's file. The
        // tab is closed instead. A drawing with a file leaves the file as it
        // is; an autosave-only one has its snapshot moved aside first, since
        // the next snapshot would otherwise prune the only copy.
        const tab = tabsRef.current.find((t) => t.id === id);
        let note = "";
        if (tab?.path) {
          note = "\n\nThe file on disk has not been changed.";
        } else {
          const kept = await keepUnreadableSnapshot(id).catch(() => null);
          if (kept) note = `\n\nIts autosave was kept at ${kept}.`;
        }
        await message(
          `${tab ? tabTitle(tab) : "A drawing"} could not be read and was closed.${note}\n\n${String(err)}`,
          { title: "Could not open tab", kind: "error" },
        );
        const remaining = tabsRef.current.filter((t) => t.id !== id);
        const next = successorId(tabsRef.current, id);
        if (remaining.length && next) {
          replaceTabs(remaining, next);
          await show(next);
        } else {
          const fresh = blankTab();
          replaceTabs([fresh], fresh.id);
          await show(fresh.id);
        }
        return;
      }
      applyScene(id, content.scene, scene, {
        view: content.view,
        // A tab restored from a session has not been parsed yet, so its saved
        // version is worked out here rather than at startup.
        savedVersion: content.savedVersion === UNPARSED ? undefined : content.savedVersion,
      });
    },
    [api, applyScene, blankTab, contentOf, replaceTabs],
  );

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
    const tab = blankTab();
    const next = [...tabsRef.current, tab];
    tabsRef.current = next;
    activeRef.current = tab.id;
    setTabs(next);
    setActiveId(tab.id);
    await show(tab.id);
  }, [blankTab, capture, show]);

  // ------------------------------------------------------------------ saving

  const writeTo = useCallback(
    async (id: string, target: string) => {
      capture();
      const content = store.current.get(id);
      if (!content) return false;
      // The version `content.scene` actually holds, taken now — in the same
      // tick as `capture()` above, before the write below awaits. Recomputing
      // this from the live scene *after* the await would let a stroke drawn
      // while the write is in flight pass as having reached disk, when only
      // the version captured here actually did.
      const activeAndCommitted = id === activeRef.current && committed.current && !!api;
      const writtenVersion = activeAndCommitted ? sceneVersion(api!.getSceneElements()) : null;
      try {
        await writeTextFile(target, content.scene);
      } catch (err) {
        await message(String(err), { title: "Could not save", kind: "error" });
        return false;
      }
      // Best effort, and deliberately after the write has been declared a
      // success: sharing the `try` above meant a failure here reported "Could
      // not save" over a drawing that had already reached disk, and left the
      // tab dirty under its old path. `openDrawing` treats it the same way.
      await pushRecent(target).catch(() => {});
      let dirty = tabsRef.current.find((tab) => tab.id === id)?.dirty ?? false;
      if (id === activeRef.current) {
        if (writtenVersion !== null) savedVersion.current = writtenVersion;
        store.current.set(id, { ...content, savedVersion: savedVersion.current });
        // Recomputed against the scene as it stands *now*, not assumed clean:
        // an edit that landed during the write above must still show dirty,
        // since disk only ever received `writtenVersion`.
        if (activeAndCommitted) dirty = sceneVersion(api!.getSceneElements()) !== savedVersion.current;
      } else {
        // Worked out the next time the tab is shown, which is the only moment
        // its elements exist as anything but text.
        store.current.set(id, { ...content, savedVersion: UNPARSED });
        dirty = false;
      }
      const next = tabsRef.current.map((tab) =>
        tab.id === id ? { ...tab, path: target, dirty } : tab,
      );
      // Written through the ref immediately, not left to the next render: a
      // quit can call `endSession` right after this resolves, and that reads
      // `tabsRef.current` directly rather than waiting for React to catch up.
      tabsRef.current = next;
      setTabs(next);
      return true;
    },
    [api, capture],
  );

  const saveTabAs = useCallback(
    async (id: string) => {
      const tab = tabsRef.current.find((t) => t.id === id);
      if (!tab) return false;
      let target: string | null;
      try {
        target = await pickSavePath("drawing", tab.path ?? "Untitled.excalidraw");
      } catch (err) {
        await message(String(err), { title: "Could not save", kind: "error" });
        return false;
      }
      if (!target) return false;
      // Two tabs on one file would each save over the other's changes.
      const other = findByPath(tabsRef.current, target);
      if (other && other.id !== id) {
        await message(
          `${tabTitle(other)} is open in another tab. Close that tab first, or save under a different name.`,
          { title: "Could not save", kind: "error" },
        );
        return false;
      }
      return writeTo(id, target);
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
      // A real third button, not just two: with only Save/Discard, Escape (or
      // the dialog's own close button) resolves to whichever one is wired as
      // "cancel" at the toolkit level — which was Discard, so dismissing the
      // dialog silently threw the tab away. Naming Cancel explicitly gives it
      // that response instead, distinguishable from actually clicking Discard.
      const outcome = await message(
        `${tabTitle(tab)} has unsaved changes. Save before continuing?`,
        {
          title: "Unsaved changes",
          kind: "warning",
          buttons: { yes: "Save", no: "Discard", cancel: "Cancel" },
        },
      );
      if (outcome === "Cancel") return false;
      return outcome === "Save" ? await saveTab(id) : true;
    },
    [saveTab],
  );

  /**
   * Held while a close or quit is asking about unsaved changes. A second one
   * arriving meanwhile (Ctrl+W again, the window's X) would stack another
   * prompt about the same drawing and could save it twice.
   */
  const prompting = useRef(false);

  const closeTab = useCallback(
    async (id?: string) => {
      if (prompting.current) return;
      prompting.current = true;
      try {
        const target = id ?? activeRef.current;
        if (!tabsRef.current.some((tab) => tab.id === target)) return;
        // Show a drawing before asking about it: a prompt naming a tab the user
        // cannot see is a prompt they cannot answer.
        if (target !== activeRef.current) await selectTab(target);
        // Showing it may have closed it already, if it could not be read.
        if (!tabsRef.current.some((tab) => tab.id === target)) return;
        if (!(await confirmTab(target))) return;

        const next = successorId(tabsRef.current, target);
        const remaining = tabsRef.current.filter((tab) => tab.id !== target);
        if (!remaining.length || !next) {
          // Closing the last tab empties the canvas rather than quitting: closing
          // a drawing and closing the app are different requests.
          const fresh = blankTab();
          replaceTabs([fresh], fresh.id);
          await show(fresh.id);
          return;
        }
        replaceTabs(remaining, next);
        await show(next);
      } finally {
        prompting.current = false;
      }
    },
    [blankTab, confirmTab, replaceTabs, selectTab, show],
  );

  const confirmDiscard = useCallback(async () => {
    if (prompting.current) return false;
    prompting.current = true;
    try {
      for (const tab of [...tabsRef.current]) {
        if (!tab.dirty) continue;
        if (tab.id !== activeRef.current) await selectTab(tab.id);
        if (!(await confirmTab(tab.id))) return false;
      }
      return true;
    } finally {
      prompting.current = false;
    }
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

  /** The snapshot being written; the next one queues behind it rather than racing it to disk. */
  const inFlight = useRef<Promise<void>>(Promise.resolve());
  /**
   * Set once `endSession` starts. A snapshot reaching disk after
   * `mark_clean_exit` would mark the exit unclean again, so from then on only
   * the final one is let through.
   */
  const ending = useRef(false);
  /** The tab list and active tab as the session file last saw them. */
  const writtenMeta = useRef("");
  /** Snapshots that have failed in a row, and whether that has been reported. */
  const failures = useRef(0);
  const warned = useRef(false);

  const snapshot = useCallback(
    (final = false): Promise<void> => {
      // Queued: two snapshots in flight could reach disk in either order, and
      // an older tab list landing last would prune the snapshot of a tab opened
      // in between — one that `written` already records as sent.
      const run = inFlight.current.then(async () => {
        if (!api || !restored.current || (ending.current && !final)) return;
        try {
          capture({ onlyIfChanged: !final });
          // The revision each scene was sent at, taken now: the tab may be
          // captured again while the write is in flight.
          const sent = new Map<string, number>();
          const payload: TabSnapshot[] = tabsRef.current.map((tab) => {
            const content = store.current.get(tab.id);
            const snap: TabSnapshot = { id: tab.id, path: tab.path, dirty: tab.dirty };
            // A drawing nobody has touched since the last snapshot is already on disk;
            // sending it again would rewrite every open tab on every keystroke.
            if (content && written.current.get(tab.id) !== content.rev) {
              snap.scene = content.scene;
              sent.set(tab.id, content.rev);
            }
            return snap;
          });
          const active = activeRef.current;
          const meta = JSON.stringify([payload.map(({ id, path, dirty }) => [id, path, dirty]), active]);
          // Excalidraw reports pointer movement as a change as well. With nothing
          // new to record, the session file is not rewritten and synced again.
          if (!sent.size && meta === writtenMeta.current) return;
          await saveSession(payload, active);
          failures.current = 0;
          writtenMeta.current = meta;
          for (const [id, rev] of sent) written.current.set(id, rev);
          for (const id of [...written.current.keys()]) {
            if (!tabsRef.current.some((tab) => tab.id === id)) written.current.delete(id);
          }
        } catch (err) {
          // Autosave is best effort; a failure here must never interrupt
          // drawing. But staying silent for the whole session left the app
          // showing unsaved marks and a tab bar that imply a snapshot exists
          // when the config directory is read-only or full, and nothing else
          // would ever mention it. Said once, and never while quitting, where
          // a dialog would only stand between the user and the exit.
          failures.current += 1;
          if (failures.current >= AUTOSAVE_FAILURES_BEFORE_WARNING && !warned.current && !ending.current) {
            warned.current = true;
            // Not awaited: the snapshot queue must not wait on a dialog.
            void message(
              "Autosave has failed several times, so this app cannot keep crash-recovery " +
                "snapshots of the drawings you have open.\n\nSaving a drawing to its own " +
                `file still works — use File → Save. Reported once per run.\n\n${String(err)}`,
              { title: "Autosave is not working", kind: "warning" },
            ).catch(() => {});
          }
        }
      });
      inFlight.current = run;
      return run;
    },
    [api, capture],
  );

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
    // Startup hasn't decided what to restore yet, so there is nothing on
    // screen worth persisting — leave the previous run's snapshot and
    // `clean_exit` flag untouched rather than overwriting real crash evidence
    // with the placeholder initial tab.
    if (!restored.current) return;
    ending.current = true;
    await snapshot(true);
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
        // Picked in Rust, which is what lets the backend read the file at all,
        // and returned canonical like a command-line path, so a file and a
        // symlink to it find the one tab `findByPath` below is meant to catch.
        let picked: string | null;
        try {
          picked = await pickOpenPath();
        } catch (err) {
          await message(String(err), { title: "Could not open file", kind: "error" });
          return;
        }
        if (!picked) return;
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
    if (!session || !session.tabs.length) {
      // Nothing to restore: still route the default tab through `show()`
      // rather than leaving it un-applied. `applyScene` merges the theme into
      // the same `updateScene` call as the (empty) content, which is the only
      // way that survives Excalidraw's own initial-mount commit landing after
      // ours — see the note on `ThemedDefaults` above. Skipping this leaves
      // the canvas coloured only by the theme effect's standalone call, which
      // loses that race unpredictably and shows Excalidraw's factory white.
      await show(activeRef.current);
      return;
    }

    // Changes the files on disk do not have only survive an unclean exit —
    // after an orderly one the user already chose to save or discard them.
    const unsaved = session.tabs.filter((tab) => tab.dirty);
    if (!session.clean_exit && unsaved.length) {
      const subject =
        unsaved.length === 1
          ? `${unsaved[0].path ? basename(unsaved[0].path) : "An unsaved drawing"} was`
          : `${unsaved.length} drawings were`;
      // Three buttons, and only an explicit Discard discards. With two, Escape
      // or closing the dialog resolved to Discard, and the next snapshot then
      // pruned work that existed nowhere else. Restoring loses nothing — the
      // tabs come back dirty and can still be closed one by one.
      const choice = await message(
        `${subject} left with unsaved changes when the app last closed. Restore them?\n\n` +
          "Only Discard throws them away.",
        {
          title: "Recover unsaved changes",
          kind: "warning",
          buttons: { yes: "Restore", no: "Discard", cancel: "Cancel" },
        },
      );
      if (choice !== "Discard") {
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
    if (!opened.length) {
      // Every tab in the session was untitled (nothing had a path to reopen).
      // Same reasoning as the empty-session case above: still push the default
      // tab through `show()` so the canvas gets its themed background from the
      // race-safe path instead of relying solely on the theme effect's own
      // standalone `updateScene` call.
      await show(activeRef.current);
      return;
    }
    const active = opened.some((t) => t.id === session.active) ? session.active! : opened[0].id;
    replaceTabs(opened, active);
    await show(active);
  }, [api, replaceTabs, show]);

  // Guarded rather than cancelled on cleanup: StrictMode runs effects twice in
  // development, and startup must not ask the user to recover twice.
  const startupDone = useRef(false);
  /** Drawings a second launch sent before startup finished, opened once it has. */
  const queued = useRef<string[]>([]);
  useEffect(() => {
    if (!api || startupDone.current) return;
    startupDone.current = true;
    void (async () => {
      try {
        // The session comes back first and the command line lands on top of it,
        // as extra tabs. Opening only the named file would leave the other tabs
        // out of the next snapshot, and the snapshot is pruned to what is open
        // — so double-clicking a drawing would quietly discard the rest.
        await restoreSession();
        for (const file of await startupFiles().catch(() => [])) {
          await openDrawing(file);
        }
        // Checked again after every await, so a drawing arriving while these
        // open is not left behind in the queue.
        while (queued.current.length) await openDrawing(queued.current.shift()!);
      } finally {
        restored.current = true;
      }
    })();
  }, [api, openDrawing, restoreSession]);

  // A second launch does not become a second app; its drawings arrive here
  // instead, as tabs on the window already open.
  useEffect(() => {
    if (!api) return;
    const pending = listen<string[]>(OPEN_FILES_EVENT, async ({ payload }) => {
      // Opened now, during startup, a tab would be thrown away again when
      // `restoreSession` replaces the whole tab set — while the recovery
      // prompt is up, say.
      if (!restored.current) {
        queued.current.push(...payload);
        return;
      }
      for (const file of payload) await openDrawing(file);
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, [api, openDrawing]);

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
  // One object for as long as its callbacks last, which is from the moment
  // Excalidraw is ready: App registers the window's close listener against it.
  const actions = useMemo<DocumentActions>(
    () => ({
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
    }),
    [newTab, openDrawing, closeTab, selectTab, selectRelative, save, saveAs, onSceneChange, confirmDiscard, endSession],
  );
  return { state, actions };
}
