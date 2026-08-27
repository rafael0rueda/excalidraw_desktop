import type { Theme } from "./types";
import { alpha, emphasis, mix } from "./color";

/**
 * Expands a theme into Excalidraw's CSS custom properties.
 *
 * Only base variables are set where possible: Excalidraw derives many of its
 * properties with `var()` (`--text-primary-color: var(--color-on-surface)`,
 * `--popup-bg-color: var(--island-bg-color)`, and so on), and those resolve
 * against the computed value on the element, so overriding the base carries the
 * whole chain with it.
 */
export function cssVariables(theme: Theme): Record<string, string> {
  const { canvas, surface, surfaceAlt, text, textMuted, accent, accentText, danger } = theme.colors;
  const dark = theme.dark;

  const accentHover = emphasis(accent, dark, 0.14);
  const accentActive = emphasis(accent, dark, 0.26);
  const accentSoft = mix(surface, accent, 0.28);

  return {
    // Text and icons. --text-primary-color and --icon-fill-color follow.
    "--color-on-surface": text,
    "--popup-text-color": text,
    "--input-label-color": textMuted,
    "--keybinding-color": textMuted,
    "--color-disabled": textMuted,

    // Panels. --popup-bg-color and --sidebar-bg-color follow --island-bg-color.
    "--island-bg-color": surface,
    "--default-bg-color": surface,
    "--input-bg-color": surface,
    "--input-hover-bg-color": surfaceAlt,
    "--popup-secondary-bg-color": surfaceAlt,
    "--overlay-bg-color": alpha(canvas, 0.88),

    // Surface ramp. --button-hover-bg, --button-active-bg,
    // --default-border-color and --sidebar-border-color all follow
    // --color-surface-high.
    "--color-surface-high": surfaceAlt,
    "--color-surface-mid": mix(surface, surfaceAlt, 0.5),
    "--color-surface-low": surface,
    "--color-surface-lowest": canvas,
    "--color-surface-primary-container": accentSoft,
    "--color-on-primary-container": text,

    // Borders.
    "--input-border-color": surfaceAlt,
    "--dialog-border-color": surfaceAlt,
    "--color-border-outline": textMuted,
    "--color-border-outline-variant": surfaceAlt,

    // Accent. --color-logo-icon and --color-promo follow --color-primary.
    "--color-primary": accent,
    "--color-primary-darker": accentHover,
    "--color-primary-darkest": accentActive,
    "--color-primary-hover": accentHover,
    "--color-primary-light": accentSoft,
    "--color-primary-light-darker": mix(surface, accent, 0.4),
    "--color-brand-hover": accentHover,
    "--color-brand-active": accentActive,
    "--color-selection": accent,
    "--select-highlight-color": accent,
    "--focus-highlight-color": alpha(accent, 0.5),
    "--link-color": accent,
    // Icons sitting on top of the accent, not literally white.
    "--color-icon-white": accentText,
    "--color-logo-text": text,

    // Destructive actions.
    "--color-danger": danger,
    "--color-danger-dark": danger,
    "--color-danger-darker": emphasis(danger, dark, 0.12),
    "--color-danger-darkest": emphasis(danger, dark, 0.24),
    "--color-danger-color": danger,
    "--color-danger-icon-color": danger,
    "--color-danger-text": text,
    "--color-danger-background": mix(surface, danger, 0.18),
    "--color-danger-icon-background": mix(surface, danger, 0.28),
    "--button-destructive-bg-color": mix(surface, danger, 0.18),
    "--button-destructive-color": danger,

    // The grey ramp, which borders and scrollbars index into. Running it from
    // the panel colour to the text colour keeps those in family with the theme
    // instead of leaving Excalidraw's light greys stranded on a dark panel.
    "--color-gray-10": mix(surface, text, 0.04),
    "--color-gray-20": mix(surface, text, 0.1),
    "--color-gray-30": mix(surface, text, 0.2),
    "--color-gray-40": mix(surface, text, 0.35),
    "--color-gray-50": mix(surface, text, 0.45),
    "--color-gray-60": mix(surface, text, 0.55),
    "--color-gray-70": mix(surface, text, 0.65),
    "--color-gray-80": mix(surface, text, 0.75),
    "--color-gray-85": mix(surface, text, 0.85),
    "--color-gray-90": mix(surface, text, 0.92),
    "--color-gray-100": text,

    "--button-gray-1": surfaceAlt,
    "--button-gray-2": mix(surfaceAlt, text, 0.2),
    "--button-gray-3": mix(surfaceAlt, text, 0.35),
    "--scrollbar-thumb": mix(surfaceAlt, text, 0.2),
    "--scrollbar-thumb-hover": mix(surfaceAlt, text, 0.4),
    "--color-slider-track": accentSoft,
    "--color-slider-thumb": accent,
  };
}
