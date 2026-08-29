import { useCallback, useEffect, useState } from "react";
import { Excalidraw } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { getCurrentWindow } from "@tauri-apps/api/window";
import TabBar from "./components/TabBar";
import ThemeEditor from "./components/ThemeEditor";
import { useDocument } from "./lib/document";
import { buildMenu, type MenuHandlers, type TabsMenu, type ThemeMenu } from "./lib/menu";
import { useTheme } from "./theme/useTheme";
import { copyToClipboard, exportPng, exportSvg } from "./lib/exportActions";

export default function App() {
  const [api, setApi] = useState<ExcalidrawImperativeAPI | null>(null);
  const [editingTheme, setEditingTheme] = useState(false);
  const theme = useTheme(api);
  const { state, actions } = useDocument(api, {
    viewBackgroundColor: theme.active.colors.canvas,
    currentItemStrokeColor: theme.active.colors.stroke,
    currentItemBackgroundColor: theme.active.colors.fill,
  });

  const handlers: MenuHandlers = {
    newTab: () => void actions.newTab(),
    closeTab: () => void actions.closeTab(),
    open: () => void actions.openDrawing(),
    openRecent: (path) => void actions.openDrawing(path),
    save: () => void actions.save(),
    saveAs: () => void actions.saveAs(),
    exportPng: () => api && void exportPng(api, state.path),
    exportPngSelection: () => api && void exportPng(api, state.path, { selectionOnly: true }),
    exportSvg: () => api && void exportSvg(api, state.path),
    copyImage: () => api && void copyToClipboard(api),
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

  const closeWindow = useCallback(async () => {
    if (!(await actions.confirmDiscard())) return;
    await actions.endSession();
    await getCurrentWindow().destroy();
  }, [actions]);

  // Rebuild the native menu whenever the bound state changes, so Save targets
  // the current path and Open Recent and Tabs stay current.
  useEffect(() => {
    if (!api) return;
    void buildMenu(handlers, themeMenu, tabsMenu);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    api,
    state.tabs,
    state.activeId,
    theme.selection,
    theme.themes,
    theme.systemPair.light?.id,
    theme.systemPair.dark?.id,
  ]);

  // Guard the window-manager close button (menu Quit routes here too).
  useEffect(() => {
    const win = getCurrentWindow();
    const pending = win.onCloseRequested(async (event) => {
      event.preventDefault();
      if (!(await actions.confirmDiscard())) return;
      await actions.endSession();
      await win.destroy();
    });
    return () => {
      void pending.then((unlisten) => unlisten());
    };
  }, [actions]);

  // Excalidraw captures many keystrokes on the canvas, so mirror the menu
  // accelerators at the window level to keep them dependable.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey) return;
      // The editor's own inputs handle their keystrokes; the menu accelerators
      // would otherwise fire while the user is typing a colour.
      if ((e.target as HTMLElement | null)?.closest(".theme-editor")) return;
      const key = e.key.toLowerCase();
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
      else if (key === "p" && shift) fire(handlers.exportPng);
      else if (key === "g" && shift) fire(handlers.exportSvg);
      else if (key === "c" && shift) fire(handlers.copyImage);
      else if (key === "," && !shift) fire(() => setEditingTheme(true));
      else if (e.key === "Tab" || e.key === "PageDown" || e.key === "PageUp") {
        const back = e.key === "PageUp" || (e.key === "Tab" && shift);
        fire(() => void actions.selectRelative(back ? -1 : 1));
      } else if (/^[1-9]$/.test(e.key) && !shift) {
        const tab = state.tabs[Number(e.key) - 1];
        if (tab) fire(() => void actions.selectTab(tab.id));
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

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
