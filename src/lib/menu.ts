import { Menu, MenuItem, PredefinedMenuItem, Submenu } from "@tauri-apps/api/menu";
import { listRecent, clearRecent, type RecentEntry } from "./api";

export interface MenuHandlers {
  newDrawing: () => void;
  open: () => void;
  openRecent: (path: string) => void;
  save: () => void;
  saveAs: () => void;
  exportPng: () => void;
  exportPngSelection: () => void;
  exportSvg: () => void;
  copyImage: () => void;
  quit: () => void;
}

async function recentSubmenu(handlers: MenuHandlers, recents: RecentEntry[]) {
  const items = recents.length
    ? await Promise.all(
        recents.map((entry, i) =>
          MenuItem.new({
            id: `recent-${i}`,
            text: entry.name,
            action: () => handlers.openRecent(entry.path),
          }),
        ),
      )
    : [await MenuItem.new({ id: "recent-empty", text: "No recent files", enabled: false })];

  if (recents.length) {
    items.push(await PredefinedMenuItem.new({ item: "Separator" }));
    items.push(
      await MenuItem.new({
        id: "recent-clear",
        text: "Clear recent files",
        action: () => {
          clearRecent().then(() => buildMenu(handlers));
        },
      }),
    );
  }
  return Submenu.new({ text: "Open Recent", items });
}

/**
 * Rebuilds and installs the whole window menu. Cheap enough to re-run whenever
 * the recent-files list changes, which keeps the submenu in sync.
 */
export async function buildMenu(handlers: MenuHandlers) {
  const recents = await listRecent().catch(() => [] as RecentEntry[]);

  const file = await Submenu.new({
    text: "File",
    items: [
      await MenuItem.new({ id: "new", text: "New", accelerator: "CmdOrCtrl+N", action: handlers.newDrawing }),
      await MenuItem.new({ id: "open", text: "Open…", accelerator: "CmdOrCtrl+O", action: handlers.open }),
      await recentSubmenu(handlers, recents),
      await PredefinedMenuItem.new({ item: "Separator" }),
      await MenuItem.new({ id: "save", text: "Save", accelerator: "CmdOrCtrl+S", action: handlers.save }),
      await MenuItem.new({ id: "saveas", text: "Save As…", accelerator: "CmdOrCtrl+Shift+S", action: handlers.saveAs }),
      await PredefinedMenuItem.new({ item: "Separator" }),
      await MenuItem.new({ id: "quit", text: "Quit", accelerator: "CmdOrCtrl+Q", action: handlers.quit }),
    ],
  });

  const exportMenu = await Submenu.new({
    text: "Export",
    items: [
      await MenuItem.new({ id: "png", text: "Export PNG…", accelerator: "CmdOrCtrl+Shift+P", action: handlers.exportPng }),
      await MenuItem.new({ id: "pngsel", text: "Export selection as PNG…", action: handlers.exportPngSelection }),
      await MenuItem.new({ id: "svg", text: "Export SVG…", accelerator: "CmdOrCtrl+Shift+G", action: handlers.exportSvg }),
      await PredefinedMenuItem.new({ item: "Separator" }),
      await MenuItem.new({ id: "copyimg", text: "Copy image to clipboard", accelerator: "CmdOrCtrl+Shift+C", action: handlers.copyImage }),
    ],
  });

  const menu = await Menu.new({ items: [file, exportMenu] });
  await menu.setAsAppMenu().catch(() => {});
  return menu;
}
