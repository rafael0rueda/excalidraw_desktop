/**
 * The key a Ctrl shortcut is matched against.
 *
 * `key` — what the layout types — is right for letters: on AZERTY the key
 * labelled Z reports `KeyW` as its code, and matching codes would turn Ctrl+Z
 * into Close Tab. But a layout that types no Latin letters (Cyrillic, Greek…)
 * leaves `key` nothing to match, and there the physical key is the only
 * sensible answer. Digits go by position in every layout — AZERTY types `&`
 * for Ctrl+1 — which is also how browsers pick tabs.
 *
 * Deliberately free of imports, so `scripts/check.mjs` can load it.
 */
export function shortcutKey(key: string, code: string): string {
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (/^[\x20-\x7e]$/.test(key)) return key.toLowerCase();
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1].toLowerCase();
  if (code === "Comma") return ",";
  // Named keys — "Tab", "PageDown" — are the same in every layout.
  return key;
}

/** What a window-level shortcut does. Each one is also a menu item. */
export type ShortcutAction =
  | "newTab"
  | "closeTab"
  | "nextTab"
  | "previousTab"
  | "open"
  | "save"
  | "saveAs"
  | "exportPng"
  | "copyImage"
  | "editTheme"
  | "quit";

export interface Shortcut {
  action: ShortcutAction;
  /** As `shortcutKey` reports it: a lower-case letter, `,`, or a named key. */
  key: string;
  /** Shift held. `"either"` for keys whose meaning Shift does not change. */
  shift: boolean | "either";
  /** The binding the menu item shows as its accelerator; a menu item takes one. */
  menu?: true;
}

/**
 * Every Ctrl shortcut the app adds, in one place. The menu takes its
 * accelerators from here and the window-level listener in `App.tsx`
 * dispatches from here. They were four hand-kept lists, and had drifted:
 * Ctrl+Q was only an accelerator, Ctrl+N and Ctrl+Tab only in the listener
 * (review 2026-11-04, ST-2). `scripts/check.mjs` holds the README's table to
 * this one. Ctrl+1 … Ctrl+9 are not here: they pick a tab by position, and
 * the menu builds them per tab.
 *
 * Ctrl+Shift+P and Ctrl+Shift+G stay Excalidraw's: command palette, ungroup.
 */
export const SHORTCUTS: readonly Shortcut[] = [
  { action: "newTab", key: "t", shift: false, menu: true },
  // With tabs there is nothing else "new" could reasonably mean, and the
  // habit is worth honouring.
  { action: "newTab", key: "n", shift: false },
  { action: "closeTab", key: "w", shift: false, menu: true },
  { action: "nextTab", key: "PageDown", shift: "either", menu: true },
  { action: "nextTab", key: "Tab", shift: false },
  { action: "previousTab", key: "PageUp", shift: "either", menu: true },
  { action: "previousTab", key: "Tab", shift: true },
  { action: "open", key: "o", shift: false, menu: true },
  { action: "save", key: "s", shift: false, menu: true },
  { action: "saveAs", key: "s", shift: true, menu: true },
  { action: "exportPng", key: "e", shift: true, menu: true },
  { action: "copyImage", key: "c", shift: true, menu: true },
  { action: "editTheme", key: ",", shift: false, menu: true },
  { action: "quit", key: "q", shift: false, menu: true },
];

/** The action Ctrl plus `key` (as `shortcutKey` reports it) is bound to, if any. */
export function shortcutAction(key: string, shift: boolean): ShortcutAction | null {
  const hit = SHORTCUTS.find((s) => s.key === key && (s.shift === "either" || s.shift === shift));
  return hit?.action ?? null;
}

function spelled(s: Shortcut, ctrl: string): string {
  const key = s.key.length === 1 ? s.key.toUpperCase() : s.key;
  return [ctrl, s.shift === true ? "Shift" : null, key].filter(Boolean).join("+");
}

/** The menu accelerator for `action`, in Tauri's spelling. */
export function accelerator(action: ShortcutAction): string {
  const s = SHORTCUTS.find((s) => s.action === action && s.menu);
  if (!s) throw new Error(`no menu shortcut for ${action}`);
  return spelled(s, "CmdOrCtrl");
}

/** How a binding is written for people: `Ctrl+Shift+S`. */
export function shortcutLabel(s: Shortcut): string {
  return spelled(s, "Ctrl");
}

/** The label of the binding the menu shows for `action`, for tooltips. */
export function menuShortcutLabel(action: ShortcutAction): string {
  const s = SHORTCUTS.find((s) => s.action === action && s.menu);
  if (!s) throw new Error(`no menu shortcut for ${action}`);
  return shortcutLabel(s);
}
