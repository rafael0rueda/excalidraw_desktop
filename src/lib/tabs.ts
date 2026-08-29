/**
 * The tab model: what the tab bar shows, and what a tab holds while it is not
 * the one on screen.
 *
 * There is one Excalidraw instance and many drawings, so a tab that is not
 * active lives here as `.excalidraw` text — the same format we save, open and
 * snapshot, which means a tab switch reuses code paths that already work rather
 * than shuttling live scene objects around behind Excalidraw's back.
 *
 * Deliberately free of runtime imports so the assertions in `scripts/check.mjs`
 * can load it without Excalidraw, React or Tauri behind it.
 */

/**
 * Scene version for a drawing that matches nothing on disk. Real scene versions
 * are sums of element versions, so they are never negative.
 */
export const NEVER_SAVED = -1;

/**
 * Scene version of a tab whose text has not been parsed yet — restored from a
 * session, never shown. Resolved from the scene itself the first time the tab
 * comes on screen, which keeps startup from parsing every drawing at once.
 */
export const UNPARSED = -2;

/** What the tab bar, the menu and the session file know about a tab. */
export interface TabMeta {
  id: string;
  path: string | null;
  dirty: boolean;
}

/** Where a tab's viewport is kept: `.excalidraw` files do not carry one. */
export interface TabView {
  scrollX: number;
  scrollY: number;
  zoom: number;
}

/** The heavy half of a tab, held outside React state so edits do not re-render. */
export interface TabContent {
  /** `.excalidraw` JSON. For the active tab this is the last capture, and so is stale by definition. */
  scene: string;
  view: TabView | null;
  savedVersion: number;
  /** Bumped whenever `scene` changes, so autosave can skip tabs that have not. */
  rev: number;
}

export function basename(path: string): string {
  return path.split("/").pop() || path;
}

export function tabTitle(tab: TabMeta): string {
  return tab.path ? basename(tab.path) : "Untitled";
}

/** A tab id: lower-case hex and dashes, which is what the backend accepts as a file name. */
export function newTabId(): string {
  return crypto.randomUUID();
}

export function findByPath(tabs: readonly TabMeta[], path: string): TabMeta | undefined {
  return tabs.find((tab) => tab.path === path);
}

/**
 * Which tab to show once `id` closes: the one that slides into its place, or
 * the one before it when the last tab is closed. Null when nothing is left.
 */
export function successorId(tabs: readonly TabMeta[], id: string): string | null {
  const index = tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return tabs[0]?.id ?? null;
  const remaining = tabs.filter((tab) => tab.id !== id);
  if (!remaining.length) return null;
  return remaining[Math.min(index, remaining.length - 1)].id;
}

/** Steps `delta` tabs from `id`, wrapping at both ends. */
export function relativeId(tabs: readonly TabMeta[], id: string, delta: number): string | null {
  if (!tabs.length) return null;
  const index = tabs.findIndex((tab) => tab.id === id);
  const from = index === -1 ? 0 : index;
  const count = tabs.length;
  return tabs[(((from + delta) % count) + count) % count].id;
}
