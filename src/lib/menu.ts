import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu } from "@tauri-apps/api/menu";
import { listRecent, clearRecent, type RecentEntry } from "./api";
import { tabTitle, type TabMeta } from "./tabs";
import { SYSTEM_THEME } from "../theme/types";

export interface MenuHandlers {
  newTab: () => void;
  closeTab: () => void;
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

/** The slice of the theme controller the menu needs. */
export interface ThemeMenu {
  themes: { id: string; name: string }[];
  /** A theme id, or "system". */
  selection: string;
  /** What "Follow system" currently resolves to, shown in the label. */
  systemLabel: string;
  select: (id: string) => void;
  reload: () => void;
  /** Opens the theme editor panel. */
  edit: () => void;
}

/** The slice of the tab controller the menu needs. */
export interface TabsMenu {
  tabs: TabMeta[];
  activeId: string;
  select: (id: string) => void;
  next: () => void;
  previous: () => void;
}

type AnyMenuItem = MenuItem | CheckMenuItem | PredefinedMenuItem;

async function recentSubmenu(
  handlers: MenuHandlers,
  theme: ThemeMenu,
  tabs: TabsMenu,
  recents: RecentEntry[],
) {
  const items: AnyMenuItem[] = recents.length
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
          clearRecent().then(() => buildMenu(handlers, theme, tabs));
        },
      }),
    );
  }
  return Submenu.new({ text: "Open Recent", items });
}

async function themeSubmenu(theme: ThemeMenu) {
  const items: AnyMenuItem[] = [
    await CheckMenuItem.new({
      id: "theme-system",
      text: `Follow system (${theme.systemLabel})`,
      checked: theme.selection === SYSTEM_THEME,
      action: () => theme.select(SYSTEM_THEME),
    }),
    await PredefinedMenuItem.new({ item: "Separator" }),
  ];

  for (const entry of theme.themes) {
    items.push(
      await CheckMenuItem.new({
        id: `theme-${entry.id}`,
        text: entry.name,
        checked: theme.selection === entry.id,
        action: () => theme.select(entry.id),
      }),
    );
  }

  items.push(await PredefinedMenuItem.new({ item: "Separator" }));
  items.push(
    await MenuItem.new({
      id: "theme-edit",
      text: "Customise themes…",
      accelerator: "CmdOrCtrl+,",
      action: theme.edit,
    }),
  );
  items.push(
    await MenuItem.new({
      id: "theme-reload",
      text: "Reload user themes",
      action: theme.reload,
    }),
  );

  return Submenu.new({ text: "Theme", items });
}

async function tabsSubmenu(tabs: TabsMenu) {
  const items: AnyMenuItem[] = [];
  for (const [index, tab] of tabs.tabs.entries()) {
    items.push(
      await CheckMenuItem.new({
        id: `tab-${index}`,
        // The same unsaved marker the tab bar shows, for the same reason: the
        // titlebar does not update on GNOME/Wayland.
        text: `${tab.dirty ? "• " : ""}${tabTitle(tab)}`,
        checked: tab.id === tabs.activeId,
        // Only the first nine get a number; beyond that the list is the way in.
        accelerator: index < 9 ? `CmdOrCtrl+${index + 1}` : undefined,
        action: () => tabs.select(tab.id),
      }),
    );
  }

  items.push(await PredefinedMenuItem.new({ item: "Separator" }));
  items.push(
    await MenuItem.new({
      id: "tab-next",
      text: "Next Tab",
      accelerator: "CmdOrCtrl+PageDown",
      action: tabs.next,
    }),
  );
  items.push(
    await MenuItem.new({
      id: "tab-previous",
      text: "Previous Tab",
      accelerator: "CmdOrCtrl+PageUp",
      action: tabs.previous,
    }),
  );

  return Submenu.new({ text: "Tabs", items });
}

/**
 * Rebuilds and installs the whole window menu. Cheap enough to re-run whenever
 * the recent-files list changes, which keeps the submenu in sync.
 */
export async function buildMenu(handlers: MenuHandlers, theme: ThemeMenu, tabs: TabsMenu) {
  const recents = await listRecent().catch(() => [] as RecentEntry[]);

  const file = await Submenu.new({
    text: "File",
    items: [
      await MenuItem.new({ id: "new", text: "New Tab", accelerator: "CmdOrCtrl+T", action: handlers.newTab }),
      await MenuItem.new({ id: "open", text: "Open…", accelerator: "CmdOrCtrl+O", action: handlers.open }),
      await recentSubmenu(handlers, theme, tabs, recents),
      await PredefinedMenuItem.new({ item: "Separator" }),
      await MenuItem.new({ id: "save", text: "Save", accelerator: "CmdOrCtrl+S", action: handlers.save }),
      await MenuItem.new({ id: "saveas", text: "Save As…", accelerator: "CmdOrCtrl+Shift+S", action: handlers.saveAs }),
      await PredefinedMenuItem.new({ item: "Separator" }),
      await MenuItem.new({ id: "close", text: "Close Tab", accelerator: "CmdOrCtrl+W", action: handlers.closeTab }),
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

  const view = await Submenu.new({ text: "View", items: [await themeSubmenu(theme)] });

  const menu = await Menu.new({ items: [file, exportMenu, view, await tabsSubmenu(tabs)] });
  await menu.setAsAppMenu().catch(() => {});
  return menu;
}
