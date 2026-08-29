import type { CSSProperties } from "react";
import type { Theme } from "./types";

/**
 * The variables our own chrome styles itself from — the tab bar and the theme
 * editor. They cannot use Excalidraw's: those are set inline on the
 * `.excalidraw` element (see `apply.ts`) and so do not reach a sibling.
 *
 * Each component sets these on its own root, which is also what lets the theme
 * editor repaint itself from a draft while the canvas still shows something
 * else.
 */
export function panelVariables(theme: Theme): CSSProperties {
  return {
    "--ui-surface": theme.colors.surface,
    "--ui-input": theme.colors.canvas,
    "--ui-hover": theme.colors.surfaceAlt,
    "--ui-border": theme.colors.surfaceAlt,
    "--ui-text": theme.colors.text,
    "--ui-muted": theme.colors.textMuted,
    "--ui-accent": theme.colors.accent,
    "--ui-accent-text": theme.colors.accentText,
    "--ui-danger": theme.colors.danger,
  } as CSSProperties;
}
