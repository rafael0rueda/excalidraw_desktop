import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { setMenuColors } from "../lib/api";
import type { Theme } from "./types";
import { cssVariables } from "./variables";

/** Properties set last time, so a theme with fewer of them cleans up after itself. */
let applied: string[] = [];

/** The vars the running theme resolves to, for repainting a root the moment it appears. */
let currentVars: Record<string, string> = {};

/** The colours the GTK chrome was last painted in, so a repaint is skipped. */
let appliedMenu = "";

function paintRoot(root: HTMLElement, vars: Record<string, string>): void {
  for (const key of applied) {
    if (!(key in vars)) root.style.removeProperty(key);
  }
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value, "important");
  }
}

/**
 * A Help, export or command-palette dialog is not nested inside the main
 * `.excalidraw` element: `Modal.tsx` portals it straight onto `<body>` as a
 * sibling carrying the `.excalidraw` class itself (see `useCreatePortalContainer`
 * in Excalidraw's source). Custom properties don't reach a sibling, and that div
 * matching `.excalidraw` on its own means Excalidraw's stylesheet reasserts its
 * own (light) defaults directly on it too — inheriting from a shared ancestor
 * can't beat a rule that targets the element itself. So each new one needs the
 * same inline override the main root gets, applied the moment it appears.
 */
let observer: MutationObserver | null = null;
function ensureObserver(): void {
  if (observer) return;
  observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        if (!(node instanceof HTMLElement)) continue;
        if (node.classList.contains("excalidraw")) paintRoot(node, currentVars);
        for (const el of node.querySelectorAll<HTMLElement>(".excalidraw")) paintRoot(el, currentVars);
      }
    }
  });
  observer.observe(document.body, { childList: true, subtree: true });
}

/**
 * The menu bar and its menus are GTK widgets rather than part of the page, so
 * no stylesheet of ours reaches them; the backend paints them instead. Called
 * from here so that a preview in the theme editor carries the menu bar with it.
 */
function paintMenuBar(theme: Theme): void {
  const colors = {
    surface: theme.colors.surface,
    text: theme.colors.text,
    muted: theme.colors.textMuted,
    accent: theme.colors.accent,
    accentText: theme.colors.accentText,
    border: theme.colors.surfaceAlt,
  };
  // Applying a theme is per-keystroke while a colour is being typed; reloading
  // a GTK style provider that often is not worth it.
  const key = Object.values(colors).join(",");
  if (key === appliedMenu) return;
  appliedMenu = key;
  // Cosmetic, and off the main path: a failure here must not stop the repaint.
  setMenuColors(colors).catch(() => {});
}

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
  // Inline + !important is the only thing that beats Excalidraw's own
  // `.excalidraw { ... }` block; a stylesheet of ours loses on specificity.
  const vars = cssVariables(theme);
  currentVars = vars;
  ensureObserver();
  for (const root of document.querySelectorAll<HTMLElement>(".excalidraw")) paintRoot(root, vars);
  applied = Object.keys(vars);

  // The gutter around the canvas, briefly visible while Excalidraw mounts.
  document.body.style.backgroundColor = theme.colors.canvas;
  paintMenuBar(theme);

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
