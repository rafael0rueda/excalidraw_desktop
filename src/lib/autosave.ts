/**
 * Crash-recovery snapshots of every open tab, and the clean-exit mark.
 *
 * Moved out of `useDocument` unchanged (review 2026-11-04, ST-4). It owns
 * nine refs that nothing else needs, and keeping them behind this boundary
 * means a callback elsewhere in the 1,000-line hook cannot reach them. The
 * two things the rest of the document does need are named below:
 * `adoptWritten` after a recovery, and `enable` once startup has decided.
 */
import { useCallback, useEffect, useRef } from "react";
import { message } from "@tauri-apps/plugin-dialog";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { markCleanExit, saveSession } from "./api";
import { nextWritten, snapshotPlan, worthWriting } from "./documentState";
import type { TabContent, TabMeta } from "./tabs";

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

export interface AutosaveInput {
  api: ExcalidrawImperativeAPI | null;
  /** Files what is on screen under the active tab; see `useDocument`. */
  capture: (opts?: { onlyIfChanged?: boolean }) => void;
  tabsRef: { readonly current: TabMeta[] };
  activeRef: { readonly current: string };
  store: { readonly current: Map<string, TabContent> };
  tabs: TabMeta[];
  activeId: string;
}

export interface Autosave {
  /** Coalesces a burst of edits into one snapshot. */
  scheduleSnapshot: () => void;
  /** The final snapshot and the clean-exit mark. */
  endSession: () => Promise<void>;
  /** Undoes `endSession` when the window turned out not to close after all. */
  resumeSession: () => void;
  /** After a recovery: the revisions the session file already holds. */
  adoptWritten: (written: Map<string, number>) => void;
  /**
   * Startup has decided what to restore. Until then, and for good if it
   * could not decide, nothing is written over the previous run's snapshots.
   */
  enable: (decided: boolean) => void;
}

export function useAutosave({
  api,
  capture,
  tabsRef,
  activeRef,
  store,
  tabs,
  activeId,
}: AutosaveInput): Autosave {
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
          // Planned before the write, so the revisions recorded afterwards are
          // the ones that actually went out: the tab may well be captured again
          // while the write is in flight.
          const tabs = tabsRef.current;
          const active = activeRef.current;
          const plan = snapshotPlan(tabs, active, (id) => store.current.get(id), written.current);
          if (!worthWriting(plan, writtenMeta.current)) return;
          await saveSession(plan.payload, active);
          failures.current = 0;
          writtenMeta.current = plan.meta;
          // Against the tabs as they stand *now*, not the list that was sent:
          // one closed while the write was in flight has had its snapshot
          // pruned already, so its revision must not linger here.
          written.current = nextWritten(written.current, plan, tabsRef.current);
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
    [api, capture, store, tabsRef, activeRef],
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

  const resumeSession = useCallback(() => {
    if (!ending.current) return;
    ending.current = false;
    // Forgotten, so the next snapshot is written whether or not anything has
    // moved: it is what takes the clean-exit mark back off the session file.
    writtenMeta.current = "";
    void snapshot();
  }, [snapshot]);


  const adoptWritten = useCallback((next: Map<string, number>) => {
    written.current = next;
  }, []);

  const enable = useCallback((decided: boolean) => {
    restored.current = decided;
  }, []);

  return { scheduleSnapshot, endSession, resumeSession, adoptWritten, enable };
}
