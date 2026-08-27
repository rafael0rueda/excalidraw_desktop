import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import type { Theme } from "./types";
import { cssVariables } from "./variables";

/** Properties set last time, so a theme with fewer of them cleans up after itself. */
let applied: string[] = [];

/**
 * Excalidraw's factory defaults. A current colour still sitting on one of these
 * was not chosen by anybody — it arrives with a reset scene, or inside a file
 * saved from stock Excalidraw — so a theme is free to replace it. Without this,
 * loading a drawing leaves new strokes black on a dark canvas: invisible.
 */
const FACTORY_STROKE = "#1e1e1e";
const FACTORY_FILL = "transparent";

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

  // Retheme the drawing defaults only while nobody has claimed them: either
  // they are still the outgoing theme's, or they are Excalidraw's own. A colour
  // the user picked by hand survives a theme change.
  const state = api.getAppState();
  const stroke = state.currentItemStrokeColor;
  const fill = state.currentItemBackgroundColor;
  const strokeIsFree =
    previous === null || stroke === previous.colors.stroke || stroke.toLowerCase() === FACTORY_STROKE;
  const fillIsFree =
    previous === null || fill === previous.colors.fill || fill.toLowerCase() === FACTORY_FILL;

  api.updateScene({
    appState: {
      viewBackgroundColor: theme.colors.canvas,
      currentItemStrokeColor: strokeIsFree ? theme.colors.stroke : stroke,
      currentItemBackgroundColor: fillIsFree ? theme.colors.fill : fill,
    },
  });
}
