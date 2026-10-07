/** Pure helpers behind the theme editor, kept out of the component so they can be tested. */
import { THEME_COLOR_KEYS, isColorValue, type Theme, type ThemeColors } from "./types";

export { TRANSPARENT, isColorValue } from "./types";

/** Turns a display name into an id the backend will accept. */
export function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

/** `base`, or `base-2`, `base-3`… until it collides with nothing. */
export function uniqueId(base: string, taken: Set<string>): string {
  const root = base || "custom";
  if (!taken.has(root)) return root;
  for (let n = 2; ; n++) {
    const candidate = `${root}-${n}`;
    if (!taken.has(candidate)) return candidate;
  }
}

/**
 * A copy of the draft that is safe to paint with. A half-typed hex would
 * otherwise reach the colour helpers, which pass unparseable values straight
 * through, and `#12` would end up in a CSS variable.
 */
export function paintable(draft: Theme, fallback: Theme): Theme {
  const colors = { ...draft.colors };
  for (const key of THEME_COLOR_KEYS) {
    if (!isColorValue(key, colors[key])) colors[key] = fallback.colors[key];
  }
  return { ...draft, colors };
}

/** Colour fields the user has left in a state we cannot save. */
export function invalidKeys(theme: Theme): (keyof ThemeColors)[] {
  return THEME_COLOR_KEYS.filter((key) => !isColorValue(key, theme.colors[key]));
}
