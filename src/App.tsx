import { useCallback, useEffect, useState } from "react";
import { Excalidraw } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useDocument } from "./lib/document";
import { buildMenu, type MenuHandlers, type ThemeMenu } from "./lib/menu";
import { useTheme } from "./theme/useTheme";
import { copyToClipboard, exportPng, exportSvg } from "./lib/exportActions";

export default function App() {
  const [api, setApi] = useState<ExcalidrawImperativeAPI | null>(null);
  const { state, actions } = useDocument(api);
  const theme = useTheme(api);

  const handlers: MenuHandlers = {
    newDrawing: () => void actions.newDrawing(),
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
  };

  const closeWindow = useCallback(async () => {
    if (!(await actions.confirmDiscard())) return;
    await actions.endSession();
    await getCurrentWindow().destroy();
  }, [actions]);

  // Rebuild the native menu whenever the bound state changes, so Save targets
  // the current path and Open Recent stays current.
  useEffect(() => {
    if (!api) return;
    void buildMenu(handlers, themeMenu);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, state.path, state.dirty, theme.selection, theme.themes]);

  // An opened file carries its own viewBackgroundColor, which would leave a
  // themed UI wrapped around an unthemed canvas. The theme wins.
  useEffect(() => {
    if (api) theme.reapply();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, state.path]);

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
      const key = e.key.toLowerCase();
      const shift = e.shiftKey;
      const fire = (fn: () => void) => {
        e.preventDefault();
        e.stopPropagation();
        fn();
      };
      if (key === "n" && !shift) fire(handlers.newDrawing);
      else if (key === "o" && !shift) fire(handlers.open);
      else if (key === "s" && !shift) fire(handlers.save);
      else if (key === "s" && shift) fire(handlers.saveAs);
      else if (key === "p" && shift) fire(handlers.exportPng);
      else if (key === "g" && shift) fire(handlers.exportSvg);
      else if (key === "c" && shift) fire(handlers.copyImage);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  return (
    <div style={{ height: "100vh", width: "100vw" }}>
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
  );
}
