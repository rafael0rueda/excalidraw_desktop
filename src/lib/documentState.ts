/**
 * The decisions behind `document.ts`, as functions over plain values.
 *
 * What is here is what the hook has to get *right* — which tabs the session
 * file still needs, what a recovered snapshot is worth, when a revision has
 * actually moved — as opposed to how it talks to React, Excalidraw and Tauri.
 * Nearly every one of these has been a bug at least once (see the review
 * sections in PROGRESS.md), and none of them could be reached from a test
 * while it lived inside the hook.
 *
 * Deliberately free of runtime imports beyond `./tabs`, so the assertions in
 * `scripts/check.mjs` can load it without Excalidraw, React or Tauri behind it.
 */
import { NEVER_SAVED, UNPARSED, basename, type TabContent, type TabMeta } from "./tabs";

/** A tab on its way into the session file. */
export interface TabSnapshot {
  id: string;
  path: string | null;
  dirty: boolean;
  /**
   * Left out when the tab's scene has not changed since the last snapshot, so
   * an autosave only ever carries the drawing actually being worked on.
   */
  scene?: string;
}

/** One tab as the session file hands it back, scene included. */
export interface SessionTab {
  id: string;
  /** File the snapshot came from, or null for a drawing never saved anywhere. */
  path: string | null;
  dirty: boolean;
  /** `.excalidraw` JSON. */
  scene: string;
}

export interface Session {
  tabs: SessionTab[];
  active: string | null;
  clean_exit: boolean;
}

// ------------------------------------------------------------------ capture

/**
 * What the active tab looked like at a capture: the tab itself, the scene
 * version (deleted elements included, so it only ever grows), the element
 * count, the canvas colour and how many files are attached.
 *
 * Serialising a scene embeds every image in it, so autosave compares this
 * first and skips the work entirely while nothing has moved.
 */
export function captureMark(
  id: string,
  version: number,
  elements: number,
  background: string,
  files: number,
): string {
  return [id, version, elements, background, files].join(":");
}

/**
 * The revision `scene` should be stored under.
 *
 * It moves only when the text does, which is what lets a snapshot leave every
 * untouched tab alone instead of rewriting all of them on each keystroke.
 */
export function nextRev(previous: TabContent | undefined, scene: string): number {
  if (previous && previous.scene === scene) return previous.rev;
  return (previous?.rev ?? 0) + 1;
}

// ----------------------------------------------------------------- autosave

export interface SnapshotPlan {
  payload: TabSnapshot[];
  /** The revision each scene went out at, to record once the write has landed. */
  sent: Map<string, number>;
  /** The tab list and active tab this payload describes. */
  meta: string;
}

/**
 * What to send the session file, given what it was last sent.
 *
 * A tab whose scene is already on disk is listed without one: the backend
 * keeps the file it has rather than rewriting identical bytes. The tab still
 * has to appear, because anything missing from the list is pruned.
 */
export function snapshotPlan(
  tabs: readonly TabMeta[],
  active: string,
  contentOf: (id: string) => TabContent | undefined,
  written: ReadonlyMap<string, number>,
): SnapshotPlan {
  const sent = new Map<string, number>();
  const payload = tabs.map((tab) => {
    const content = contentOf(tab.id);
    const snap: TabSnapshot = { id: tab.id, path: tab.path, dirty: tab.dirty };
    if (content && written.get(tab.id) !== content.rev) {
      snap.scene = content.scene;
      sent.set(tab.id, content.rev);
    }
    return snap;
  });
  const meta = JSON.stringify([payload.map(({ id, path, dirty }) => [id, path, dirty]), active]);
  return { payload, sent, meta };
}

/**
 * Whether a plan is worth writing. Excalidraw reports pointer movement as a
 * change too, and rewriting and fsyncing the session file for that is pure
 * cost — but a tab opened, closed, renamed or switched to has to be recorded
 * even though no scene moved with it.
 */
export function worthWriting(plan: SnapshotPlan, lastMeta: string): boolean {
  return plan.sent.size > 0 || plan.meta !== lastMeta;
}

/**
 * `written`, updated for a plan that reached disk.
 *
 * Tabs that have since closed are dropped: their snapshots are pruned by the
 * backend, so a stale revision here would let a reopened id be mistaken for
 * one already on disk.
 */
export function nextWritten(
  written: ReadonlyMap<string, number>,
  plan: SnapshotPlan,
  tabs: readonly TabMeta[],
): Map<string, number> {
  const next = new Map(written);
  for (const [id, rev] of plan.sent) next.set(id, rev);
  for (const id of [...next.keys()]) {
    if (!tabs.some((tab) => tab.id === id)) next.delete(id);
  }
  return next;
}

// ------------------------------------------------------------------ startup

/**
 * Which tab to put on screen: the one the session named, if it is still among
 * these, else the first. Null only when there are no tabs at all.
 */
export function activeFrom(tabs: readonly TabMeta[], preferred: string | null): string | null {
  const named = tabs.find((tab) => tab.id === preferred);
  return named ? named.id : (tabs[0]?.id ?? null);
}

/**
 * How to describe a session's unsaved work, or null when there is nothing to
 * ask the user about.
 *
 * Only an unclean exit is worth a prompt: after an orderly one they were asked
 * about every dirty tab already and answered.
 */
export function recoverySubject(session: Session): string | null {
  if (session.clean_exit) return null;
  const unsaved = session.tabs.filter((tab) => tab.dirty);
  if (!unsaved.length) return null;
  if (unsaved.length === 1) {
    return `${unsaved[0].path ? basename(unsaved[0].path) : "An unsaved drawing"} was`;
  }
  return `${unsaved.length} drawings were`;
}

/**
 * The tabs and content a "Restore" answer produces.
 *
 * A recovered drawing matches nothing on disk, so it keeps its unsaved mark
 * until the user actually saves it. One that was clean is left `UNPARSED` and
 * works its own version out the first time it comes on screen. Every tab
 * starts at revision 1, and `written` says so too: the session file already
 * holds exactly these scenes, so the first snapshot after a restore has
 * nothing to send and must not rewrite them all.
 */
export function recoveredSession(session: Session): {
  tabs: TabMeta[];
  contents: Map<string, TabContent>;
  written: Map<string, number>;
  active: string | null;
} {
  const tabs = session.tabs.map(({ id, path, dirty }) => ({ id, path, dirty }));
  const contents = new Map<string, TabContent>();
  const written = new Map<string, number>();
  for (const tab of session.tabs) {
    contents.set(tab.id, {
      scene: tab.scene,
      // No stored view: the snapshot's own appState puts the user back at the
      // viewport they were working in.
      view: null,
      savedVersion: tab.dirty ? NEVER_SAVED : UNPARSED,
      rev: 1,
    });
    written.set(tab.id, 1);
  }
  return { tabs, contents, written, active: activeFrom(tabs, session.active) };
}

/** The content a tab reopened from its own file starts with. */
export function reopenedContent(scene: string): TabContent {
  // UNPARSED: its version is worked out the first time the tab is shown, which
  // is the only moment its elements exist as anything but text.
  return { scene, view: null, savedVersion: UNPARSED, rev: 1 };
}
