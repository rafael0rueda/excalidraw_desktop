import { isHex } from "./color";

/**
 * A theme is deliberately small and hand-authorable: ten colours the user can
 * reason about, expanded into Excalidraw's ~30 CSS custom properties by
 * `variables.ts`. Anything the user would have to look up in Excalidraw's
 * stylesheet does not belong here.
 */
export interface ThemeColors {
  /** Drawing surface behind the elements. */
  canvas: string;
  /** Toolbars, panels, dialogs. */
  surface: string;
  /** Hovers, borders, secondary panels. */
  surfaceAlt: string;
  text: string;
  textMuted: string;
  accent: string;
  /** Text and icons drawn *on top of* `accent`. */
  accentText: string;
  danger: string;
  /** Stroke colour given to newly drawn elements. */
  stroke: string;
  /** Fill given to newly drawn elements; usually "transparent". */
  fill: string;
}

export interface Theme {
  id: string;
  name: string;
  /**
   * Whether this is a dark colour scheme. Drives contrast decisions (which way
   * hover states move) and which slot the theme fills when following the
   * desktop. It does *not* switch Excalidraw into its own dark mode — that
   * inverts the canvas. See PROGRESS.md.
   */
  dark: boolean;
  colors: ThemeColors;
}

export const THEME_COLOR_KEYS: (keyof ThemeColors)[] = [
  "canvas",
  "surface",
  "surfaceAlt",
  "text",
  "textMuted",
  "accent",
  "accentText",
  "danger",
  "stroke",
  "fill",
];

/** The one non-colour value a field may hold, and only `fill` may hold it. */
export const TRANSPARENT = "transparent";

/**
 * The one grammar for a theme colour: `#rgb` or `#rrggbb`, and `transparent`
 * for the fill. The editor, the loader and the backend's `css_color` used to
 * disagree, so a value one of them accepted painted the page and left the
 * menu bar behind, or the other way round.
 */
export function isColorValue(key: keyof ThemeColors, value: string): boolean {
  return isHex(value) || (key === "fill" && value === TRANSPARENT);
}

/** What the backend's `safe_id` accepts: the id is also the theme's file name. */
const THEME_ID = /^[a-z0-9-]{1,64}$/;

/** Follow the desktop's light/dark preference instead of pinning one theme. */
export const SYSTEM_THEME = "system";

/**
 * Validates a theme loaded from disk. User themes are hand-written files, so a
 * missing key is expected rather than exceptional — we say what is wrong and
 * skip the file instead of failing to start.
 */
export function parseTheme(value: unknown): { theme: Theme } | { error: string } {
  if (typeof value !== "object" || value === null) return { error: "not a JSON object" };
  const raw = value as Record<string, unknown>;

  if (typeof raw.id !== "string" || !raw.id) return { error: "missing \"id\"" };
  if (!THEME_ID.test(raw.id)) {
    return { error: `${raw.id}: "id" may only use lower-case letters, digits and dashes` };
  }
  if (typeof raw.name !== "string" || !raw.name.trim()) return { error: `${raw.id}: missing "name"` };

  const colors = raw.colors;
  if (typeof colors !== "object" || colors === null) return { error: `${raw.id}: missing "colors"` };

  const out = {} as ThemeColors;
  for (const key of THEME_COLOR_KEYS) {
    const colour = (colors as Record<string, unknown>)[key];
    if (typeof colour !== "string" || !colour) return { error: `${raw.id}: missing "colors.${key}"` };
    if (!isColorValue(key, colour)) {
      const also = key === "fill" ? `, or "${TRANSPARENT}"` : "";
      return { error: `${raw.id}: "colors.${key}" is not a colour — use #rgb or #rrggbb${also}` };
    }
    out[key] = colour;
  }

  return { theme: { id: raw.id, name: raw.name, dark: raw.dark === true, colors: out } };
}

/**
 * A theme as it is written to disk: schema order, two-space indent, trailing
 * newline. These files are hand-edited, and the documented order groups the
 * colours by role — alphabetical would scatter those groups and put the whole
 * `colors` block above the `id` that names it.
 */
export function serializeTheme(theme: Theme): string {
  const colors = {} as ThemeColors;
  for (const key of THEME_COLOR_KEYS) colors[key] = theme.colors[key];
  const ordered = { id: theme.id, name: theme.name, dark: theme.dark, colors };
  return `${JSON.stringify(ordered, null, 2)}\n`;
}
