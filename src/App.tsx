import { useCallback, useEffect, useRef, useState } from "react";
import { Excalidraw } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { getCurrentWindow } from "@tauri-apps/api/window";
import TabBar from "./components/TabBar";
import ThemeEditor from "./components/ThemeEditor";
import { useDocument } from "./lib/document";
import {
  buildMenu,
  updateTabMarks,
  type ExportMenu,
  type MenuHandlers,
  type TabsMenu,
  type ThemeMenu,
} from "./lib/menu";
import { useTheme } from "./theme/useTheme";
import { copyToClipboard, exportPng, exportSvg } from "./lib/exportActions";
import type { ExportOptions } from "./lib/exports";
import { useExportPreferences } from "./lib/exportPreferences";
import { addErShapes, usePersistentLibrary } from "./lib/library";
import { openLink } from "./lib/links";
import { shortcutKey } from "./lib/shortcuts";

export default function App() {
  const [api, setApi] = useState<ExcalidrawImperativeAPI | null>(null);
  const [editingTheme, setEditingTheme] = useState(false);
  const theme = useTheme(api);
  const { state, actions } = useDocument(api, {
    viewBackgroundColor: theme.active.colors.canvas,
    currentItemStrokeColor: theme.active.colors.stroke,
    currentItemBackgroundColor: theme.active.colors.fill,
  });
  const exportPrefs = useExportPreferences();
  usePersistentLibrary(api);

  // The window starts hidden (`visible: false` in tauri.conf.json) and is shown
  // once the theme has been read and painted, so a dark theme does not open as
  // a white flash. lib.rs shows it anyway if this never runs.
  useEffect(() => {
    if (api && theme.ready) void getCurrentWindow().show().catch(() => {});
  }, [api, theme.ready]);

  // Read when an export runs rather than captured when the menu was built: a
  // tab switch changes the active path without rebuilding the menu.
  const activePath = useRef(state.path);
  activePath.current = state.path;
  const preferences = useRef(exportPrefs.preferences);
  preferences.current = exportPrefs.preferences;
  const exportOptions = (): ExportOptions => ({
    scale: preferences.current.scale,
    transparent: preferences.current.transparent,
    embedScene: preferences.current.embed_scene,
  });

  const handlers: MenuHandlers = {
    newTab: () => void actions.newTab(),
    closeTab: () => void actions.closeTab(),
    open: () => void actions.openDrawing(),
    openRecent: (path) => void actions.openDrawing(path),
    save: () => void actions.save(),
    saveAs: () => void actions.saveAs(),
    exportPng: () => api && void exportPng(api, activePath.current, exportOptions()),
    exportPngSelection: () =>
      api && void exportPng(api, activePath.current, { ...exportOptions(), selectionOnly: true }),
    exportSvg: () => api && void exportSvg(api, activePath.current, exportOptions()),
    copyImage: () => api && void copyToClipboard(api, exportOptions()),
    addErShapes: () => api && void addErShapes(api),
    quit: () => void closeWindow(),
  };

  const themeMenu: ThemeMenu = {
    themes: theme.themes,
    selection: theme.selection,
    systemLabel: `${theme.systemPair.light?.name ?? "—"} / ${theme.systemPair.dark?.name ?? "—"}`,
    select: theme.select,
    reload: () => void theme.reload(),
    edit: () => setEditingTheme(true),
  };

  const tabsMenu: TabsMenu = {
    tabs: state.tabs,
    activeId: state.activeId,
    select: (id) => void actions.selectTab(id),
    next: () => void actions.selectRelative(1),
    previous: () => void actions.selectRelative(-1),
  };

  const exportMenu: ExportMenu = {
    scale: exportPrefs.preferences.scale,
    transparent: exportPrefs.preferences.transparent,
    embedScene: exportPrefs.preferences.embed_scene,
    setScale: (scale) => exportPrefs.update({ scale }),
    setTransparent: (transparent) => exportPrefs.update({ transparent }),
    setEmbedScene: (embed_scene) => exportPrefs.update({ embed_scene }),
  };

  // One quit at a time: Ctrl+Q again, or the window's X while the first quit's
  // prompt is still up, would otherwise run a second one alongside it.
  const quitting = useRef(false);
  const closeWindow = useCallback(async () => {
    if (quitting.current) return;
    quitting.current = true;
    try {
      if (!(await actions.confirmDiscard())) return;
      await actions.endSession();
      await getCurrentWindow().destroy();
    } finally {
      quitting.current = false;
    }
  }, [actions]);

  // Rebuild the native menu when an item has to appear, go, be renamed or be
  // checked: a tab opened, closed or saved under a new name, the themes, or
  // the export choices changing.
  const tabShape = state.tabs.map((tab) => `${tab.id}:${tab.path ?? ""}`).join("\n");
  useEffect(() => {
    if (!api) return;
    void buildMenu(handlers, themeMenu, tabsMenu, exportMenu);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    api,
    tabShape,
    theme.selection,
    theme.themes,
    theme.systemPair.light?.id,
    theme.systemPair.dark?.id,
    exportPrefs.preferences,
  ]);

  // Unsaved marks and the active tab change far more often, and are updated in place.
  const tabMarks = `${state.activeId}:${state.tabs.map((tab) => (tab.dirty ? "1" : "0")).join("")}`;
  useEffect(() => {
    if (!api) return;
    void updateTabMarks(tabsMenu);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, tabMarks]);

  // Guard the window-manager close button (menu Quit routes here too).
  useEffect(() => {
    const pending = getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault();
      void closeWindow();
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, [closeWindow]);

  // Read through a ref, so the listener below is added once rather than on every render.
  const keys = useRef({ handlers, actions, tabs: state.tabs });
  keys.current = { handlers, actions, tabs: state.tabs };

  // Excalidraw captures many keystrokes on the canvas, so mirror the menu
  // accelerators at the window level to keep them dependable.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey) return;
      // The editor's own inputs handle their keystrokes; the menu accelerators
      // would otherwise fire while the user is typing a colour.
      if ((e.target as HTMLElement | null)?.closest(".theme-editor")) return;
      const { handlers, actions, tabs } = keys.current;
      const key = shortcutKey(e.key, e.code);
      const shift = e.shiftKey;
      const fire = (fn: () => void) => {
        e.preventDefault();
        e.stopPropagation();
        fn();
      };
      // Ctrl+N opens a tab as well as Ctrl+T: with tabs there is nothing else
      // "new" could reasonably mean, and the habit is worth honouring.
      if ((key === "t" || key === "n") && !shift) fire(handlers.newTab);
      else if (key === "w" && !shift) fire(handlers.closeTab);
      else if (key === "o" && !shift) fire(handlers.open);
      else if (key === "s" && !shift) fire(handlers.save);
      else if (key === "s" && shift) fire(handlers.saveAs);
      // Ctrl+Shift+P and Ctrl+Shift+G stay Excalidraw's: command palette, ungroup.
      else if (key === "e" && shift) fire(handlers.exportPng);
      else if (key === "c" && shift) fire(handlers.copyImage);
      else if (key === "," && !shift) fire(() => setEditingTheme(true));
      else if (e.key === "Tab" || e.key === "PageDown" || e.key === "PageUp") {
        const back = e.key === "PageUp" || (e.key === "Tab" && shift);
        fire(() => void actions.selectRelative(back ? -1 : 1));
      } else if (/^[1-9]$/.test(key) && !shift) {
        const tab = tabs[Number(key) - 1];
        if (tab) fire(() => void actions.selectTab(tab.id));
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <div style={{ height: "100vh", width: "100vw", display: "flex", flexDirection: "column" }}>
      <TabBar
        tabs={state.tabs}
        activeId={state.activeId}
        theme={theme.active}
        onSelect={(id) => void actions.selectTab(id)}
        onClose={(id) => void actions.closeTab(id)}
        onNew={() => void actions.newTab()}
      />
      {/* minHeight lets the canvas shrink inside the column rather than
          overflowing it, which is what a flex item does by default. */}
      <div style={{ flex: 1, minHeight: 0, position: "relative" }}>
        <Excalidraw
          excalidrawAPI={setApi}
          // Always "light": Excalidraw's dark mode inverts the canvas, so dark
          // themes are built from dark colours on the light base instead.
          theme="light"
          onChange={actions.onSceneChange}
          onLinkOpen={(element, event) => {
            // Followed by us rather than in the webview; see `openLink`.
            event.preventDefault();
            if (api && element.link) openLink(api, element.link);
          }}
          UIOptions={{
            canvasActions: {
              loadScene: false,
              saveToActiveFile: false,
              export: false,
              saveAsImage: false,
              toggleTheme: true,
              clearCanvas: true,
              changeViewBackgroundColor: true,
            },
          }}
        />
      </div>
      {editingTheme && <ThemeEditor theme={theme} onClose={() => setEditingTheme(false)} />}
    </div>
  );
}
