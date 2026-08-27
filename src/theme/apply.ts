import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import type { Theme } from "./types";
import { cssVariables } from "./variables";

/** Properties set last time, so a theme with fewer of them cleans up after itself. */
let applied: string[] = [];

/**
 * Paints a theme onto the running Excalidraw instance.
 *
 * `previous` is the theme currently in effect, and exists so a switch does not
 * quietly discard a colour the user picked by hand.
 */
export function applyTheme(
  theme: Theme,
  api: ExcalidrawImperativeAPI | null,
  previous: Theme | null,
): void {
  const root = document.querySelector<HTMLElement>(".excalidraw");
  if (root) {
    // Inline + !important is the only thing that beats Excalidraw's own
    // `.excalidraw { ... }` block; a stylesheet of ours loses on specificity.
    const vars = cssVariables(theme);
    for (const key of applied) {
      if (!(key in vars)) root.style.removeProperty(key);
    }
    for (const [key, value] of Object.entries(vars)) {
      root.style.setProperty(key, value, "important");
    }
    applied = Object.keys(vars);
  }

  // The gutter around the canvas, briefly visible while Excalidraw mounts.
  document.body.style.backgroundColor = theme.colors.canvas;

  if (!api) return;

  // Retheme the drawing defaults only while they are still the outgoing
  // theme's — once the user has chosen a colour, it is theirs to keep.
  const state = api.getAppState();
  const ownStroke = previous !== null && state.currentItemStrokeColor !== previous.colors.stroke;
  const ownFill = previous !== null && state.currentItemBackgroundColor !== previous.colors.fill;

  api.updateScene({
    appState: {
      viewBackgroundColor: theme.colors.canvas,
      currentItemStrokeColor: ownStroke ? state.currentItemStrokeColor : theme.colors.stroke,
      currentItemBackgroundColor: ownFill ? state.currentItemBackgroundColor : theme.colors.fill,
    },
  });
}
