import type { Resource } from "@tauri-apps/api/core";
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

/** A Tabs menu entry, kept so its marks can change without a rebuild. */
interface TabEntry {
  id: string;
  item: CheckMenuItem;
}

/**
 * Every native item behind the installed menu. Tauri holds each one in a
 * resource table until it is closed, so a rebuild that dropped the last menu
 * without closing it grew that table every time.
 */
let installed: Resource[] = [];
let tabEntries: TabEntry[] = [];
/** The latest tab state either export was handed. */
let latestTabs: TabsMenu | null = null;
/** Moves with every build, so an older build that finishes late is dropped rather than installed. */
let generation = 0;

/** Awaits a new item and records it in `made`, to be closed along with its menu. */
async function keep<T extends Resource>(made: Resource[], created: Promise<T>): Promise<T> {
  const item = await created;
  made.push(item);
  return item;
}

function closeAll(resources: Resource[]) {
  return Promise.allSettled(resources.map((resource) => resource.close()));
}

/**
 * The same unsaved marker the tab bar shows, for the same reason: the
 * titlebar does not update on GNOME/Wayland.
 */
function tabLabel(tab: TabMeta): string {
  return `${tab.dirty ? "• " : ""}${tabTitle(tab)}`;
}

async function recentSubmenu(
  made: Resource[],
  handlers: MenuHandlers,
  theme: ThemeMenu,
  tabs: TabsMenu,
  recents: RecentEntry[],
) {
  const items: AnyMenuItem[] = recents.length
    ? await Promise.all(
        recents.map((entry, i) =>
          keep(
            made,
            MenuItem.new({
              id: `recent-${i}`,
              text: entry.name,
              action: () => handlers.openRecent(entry.path),
            }),
          ),
        ),
      )
    : [await keep(made, MenuItem.new({ id: "recent-empty", text: "No recent files", enabled: false }))];

  if (recents.length) {
    items.push(await keep(made, PredefinedMenuItem.new({ item: "Separator" })));
    items.push(
      await keep(
        made,
        MenuItem.new({
          id: "recent-clear",
          text: "Clear recent files",
          action: () => {
            void clearRecent().then(() => buildMenu(handlers, theme, latestTabs ?? tabs));
          },
        }),
      ),
    );
  }
  return keep(made, Submenu.new({ text: "Open Recent", items }));
}

async function themeSubmenu(made: Resource[], theme: ThemeMenu) {
  const items: AnyMenuItem[] = [
    await keep(
      made,
      CheckMenuItem.new({
        id: "theme-system",
        text: `Follow system (${theme.systemLabel})`,
        checked: theme.selection === SYSTEM_THEME,
        action: () => theme.select(SYSTEM_THEME),
      }),
    ),
    await keep(made, PredefinedMenuItem.new({ item: "Separator" })),
  ];

  for (const entry of theme.themes) {
    items.push(
      await keep(
        made,
        CheckMenuItem.new({
          id: `theme-${entry.id}`,
          text: entry.name,
          checked: theme.selection === entry.id,
          action: () => theme.select(entry.id),
        }),
      ),
    );
  }

  items.push(await keep(made, PredefinedMenuItem.new({ item: "Separator" })));
  items.push(
    await keep(
      made,
      MenuItem.new({
        id: "theme-edit",
        text: "Customise themes…",
        accelerator: "CmdOrCtrl+,",
        action: theme.edit,
      }),
    ),
  );
  items.push(
    await keep(
      made,
      MenuItem.new({
        id: "theme-reload",
        text: "Reload user themes",
        action: theme.reload,
      }),
    ),
  );

  return keep(made, Submenu.new({ text: "Theme", items }));
}

async function tabsSubmenu(made: Resource[], tabs: TabsMenu, entries: TabEntry[]) {
  const items: AnyMenuItem[] = [];
  for (const [index, tab] of tabs.tabs.entries()) {
    const item = await keep(
      made,
      CheckMenuItem.new({
        id: `tab-${index}`,
        text: tabLabel(tab),
        checked: tab.id === tabs.activeId,
        // Only the first nine get a number; beyond that the list is the way in.
        accelerator: index < 9 ? `CmdOrCtrl+${index + 1}` : undefined,
        action: () => tabs.select(tab.id),
      }),
    );
    entries.push({ id: tab.id, item });
    items.push(item);
  }

  items.push(await keep(made, PredefinedMenuItem.new({ item: "Separator" })));
  items.push(
    await keep(
      made,
      MenuItem.new({
        id: "tab-next",
        text: "Next Tab",
        accelerator: "CmdOrCtrl+PageDown",
        action: tabs.next,
      }),
    ),
  );
  items.push(
    await keep(
      made,
      MenuItem.new({
        id: "tab-previous",
        text: "Previous Tab",
        accelerator: "CmdOrCtrl+PageUp",
        action: tabs.previous,
      }),
    ),
  );

  return keep(made, Submenu.new({ text: "Tabs", items }));
}

/**
 * Brings the Tabs menu's unsaved marks and check mark up to date in place. The
 * first edit after every save flips a mark, and rebuilding the whole menu for
 * that recreated every item in it.
 */
export async function updateTabMarks(tabs: TabsMenu) {
  latestTabs = tabs;
  await Promise.allSettled(
    tabEntries.flatMap(({ id, item }) => {
      const tab = tabs.tabs.find((t) => t.id === id);
      return tab ? [item.setText(tabLabel(tab)), item.setChecked(tab.id === tabs.activeId)] : [];
    }),
  );
}

/**
 * Rebuilds and installs the whole window menu, for when an item has to appear,
 * disappear or be renamed. The menu it replaces is closed.
 */
export async function buildMenu(handlers: MenuHandlers, theme: ThemeMenu, tabs: TabsMenu) {
  const build = ++generation;
  latestTabs = tabs;
  const made: Resource[] = [];
  const entries: TabEntry[] = [];
  try {
    const recents = await listRecent().catch(() => [] as RecentEntry[]);

    const file = await keep(
      made,
      Submenu.new({
        text: "File",
        items: [
          await keep(made, MenuItem.new({ id: "new", text: "New Tab", accelerator: "CmdOrCtrl+T", action: handlers.newTab })),
          await keep(made, MenuItem.new({ id: "open", text: "Open…", accelerator: "CmdOrCtrl+O", action: handlers.open })),
          await recentSubmenu(made, handlers, theme, tabs, recents),
          await keep(made, PredefinedMenuItem.new({ item: "Separator" })),
          await keep(made, MenuItem.new({ id: "save", text: "Save", accelerator: "CmdOrCtrl+S", action: handlers.save })),
          await keep(made, MenuItem.new({ id: "saveas", text: "Save As…", accelerator: "CmdOrCtrl+Shift+S", action: handlers.saveAs })),
          await keep(made, PredefinedMenuItem.new({ item: "Separator" })),
          await keep(made, MenuItem.new({ id: "close", text: "Close Tab", accelerator: "CmdOrCtrl+W", action: handlers.closeTab })),
          await keep(made, MenuItem.new({ id: "quit", text: "Quit", accelerator: "CmdOrCtrl+Q", action: handlers.quit })),
        ],
      }),
    );

    // Not Ctrl+Shift+P or Ctrl+Shift+G: Excalidraw binds those to its command
    // palette and to ungroup. Ctrl+Shift+E is its own image-export key, which
    // does nothing here since that dialog is turned off.
    const exportMenu = await keep(
      made,
      Submenu.new({
        text: "Export",
        items: [
          await keep(made, MenuItem.new({ id: "png", text: "Export PNG…", accelerator: "CmdOrCtrl+Shift+E", action: handlers.exportPng })),
          await keep(made, MenuItem.new({ id: "pngsel", text: "Export selection as PNG…", action: handlers.exportPngSelection })),
          await keep(made, MenuItem.new({ id: "svg", text: "Export SVG…", action: handlers.exportSvg })),
          await keep(made, PredefinedMenuItem.new({ item: "Separator" })),
          await keep(made, MenuItem.new({ id: "copyimg", text: "Copy image to clipboard", accelerator: "CmdOrCtrl+Shift+C", action: handlers.copyImage })),
        ],
      }),
    );

    const view = await keep(made, Submenu.new({ text: "View", items: [await themeSubmenu(made, theme)] }));

    const menu = await keep(
      made,
      Menu.new({ items: [file, exportMenu, view, await tabsSubmenu(made, tabs, entries)] }),
    );
    if (build !== generation) {
      await closeAll(made);
      return;
    }
    const previous = await menu.setAsAppMenu().catch(() => null);
    // A fresh handle on the menu just replaced, and a resource of its own.
    if (previous) void previous.close().catch(() => {});
    const replaced = installed;
    installed = made;
    tabEntries = entries;
    await closeAll(replaced);
    // Marks that changed while this was being built.
    if (latestTabs && latestTabs !== tabs) await updateTabMarks(latestTabs);
  } catch {
    if (installed !== made) await closeAll(made);
  }
}
