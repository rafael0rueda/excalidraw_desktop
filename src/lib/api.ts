// Typed wrappers around the Rust command layer. The renderer never touches the
// filesystem directly — everything goes through these invocations.
import { invoke } from "@tauri-apps/api/core";

export interface RecentEntry {
  path: string;
  name: string;
  opened_at: number;
}

export const readTextFile = (path: string) => invoke<string>("read_text_file", { path });

/**
 * The native Open dialog, run in Rust because the backend only reads files the
 * user chose. Resolves to the canonical path, or null if the user cancelled.
 */
export const pickOpenPath = () => invoke<string | null>("pick_open_path");

export const writeTextFile = (path: string, contents: string) =>
  invoke<void>("write_text_file", { path, contents });

/**
 * Sends the PNG as the raw request body; JSON, whether a number array or
 * base64, is slow for multi-megabyte images. The path goes in a header, base64
 * so that any file name survives header encoding.
 */
export const writeBinaryFile = (path: string, data: Uint8Array) =>
  invoke<void>("write_binary_file", data, {
    headers: { "x-path": bytesToBase64(new TextEncoder().encode(path)) },
  });

export type SaveKind = "drawing" | "png" | "svg";

/**
 * The native save dialog. Resolves to a path that already ends in the kind's
 * extension, or null if the user cancelled. When the extension had to be
 * added, an existing file of that name has been asked about first — the
 * dialog itself only checked the name as typed.
 */
export const pickSavePath = (kind: SaveKind, suggested: string) =>
  invoke<string | null>("pick_save_path", { kind, suggested });

/** `data` is a PNG, sent as the raw request body. */
export const copyImageToClipboard = (data: Uint8Array) => invoke<void>("copy_image_to_clipboard", data);

/** The six colours the GTK menu bar and its menus are painted in. */
export interface MenuColors {
  surface: string;
  text: string;
  muted: string;
  accent: string;
  accentText: string;
  border: string;
}

export const setMenuColors = (colors: MenuColors) => invoke<void>("set_menu_colors", { colors });

/**
 * Sets GTK's own dark/light preference to match the active theme, so a native
 * dialog (the file chooser Library's "Load from file" raises most of all)
 * renders correctly the first time rather than racing GTK's own portal query.
 */
export const setPreferDarkTheme = (dark: boolean) =>
  invoke<void>("set_prefer_dark_theme", { dark });

export const listRecent = () => invoke<RecentEntry[]>("list_recent");
export const pushRecent = (path: string) => invoke<RecentEntry[]>("push_recent", { path });
export const clearRecent = () => invoke<RecentEntry[]>("clear_recent");

export interface SessionTab {
  id: string;
  /** File the snapshot came from, or null for a drawing never saved anywhere. */
  path: string | null;
  dirty: boolean;
  /** `.excalidraw` JSON. */
  scene: string;
}

export interface Session {
  tabs: SessionTab[];
  active: string | null;
  clean_exit: boolean;
}

/** A tab on its way into the session file. */
export interface TabSnapshot {
  id: string;
  path: string | null;
  dirty: boolean;
  /**
   * Left out when the tab's scene has not changed since the last snapshot, so
   * an autosave only ever carries the drawing actually being worked on.
   */
  scene?: string;
}

export const saveSession = (tabs: TabSnapshot[], active: string | null) =>
  invoke<void>("save_session", { tabs, active });
export const loadSession = () => invoke<Session | null>("load_session");
export const markCleanExit = () => invoke<void>("mark_clean_exit");
/** Moves an unparseable snapshot out of the way of pruning; resolves to where it went. */
export const keepUnreadableSnapshot = (id: string) =>
  invoke<string>("keep_unreadable_snapshot", { id });

/** Mirrors the Rust `Settings` struct, snake_case included. */
export interface Settings {
  /** A theme id, or "system" to follow the desktop's light/dark preference. */
  theme: string;
  light_theme: string;
  dark_theme: string;
}

export const loadSettings = () => invoke<Settings>("load_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const themesDirPath = () => invoke<string>("themes_dir_path");

/**
 * One file from ~/.config/excalidraw-desktop/themes: the raw JSON it held, or
 * why it could not even be read or parsed as JSON. The caller validates the
 * schema of `value` itself.
 */
export interface ThemeFile {
  value: unknown | null;
  error: string | null;
}
export const listUserThemes = () => invoke<ThemeFile[]>("list_user_themes");
export const systemColorScheme = () => invoke<"light" | "dark">("system_color_scheme");
/**
 * Writes `<id>.json` into the themes directory; resolves to the file written.
 * The caller supplies the finished text so that its key order survives — see
 * `serializeTheme`.
 */
export const saveUserTheme = (id: string, contents: string) =>
  invoke<string>("save_user_theme", { id, contents });
export const deleteUserTheme = (id: string) => invoke<void>("delete_user_theme", { id });

/**
 * Paths passed on the command line, e.g. from a file-manager double click. The
 * desktop entry's `%F` can hand over several drawings in one launch, and the
 * list empties on the first call so a reload does not reopen them.
 */
export const startupFiles = () => invoke<string[]>("startup_files");

/** Drawings sent over by a second launch; see the single-instance plugin. */
export const OPEN_FILES_EVENT = "open-files";

export const setWindowTitle = (title: string) => invoke<void>("set_window_title", { title });

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000; // avoid blowing the argument limit on large buffers
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** Sent by the backend when the desktop switches between light and dark. */
export const COLOR_SCHEME_EVENT = "color-scheme-changed";

/** The saved `.excalidrawlib` text, or null if no library was ever saved. */
export const loadLibrary = () => invoke<string | null>("load_library");
export const saveLibrary = (contents: string) => invoke<void>("save_library", { contents });

/** Mirrors the Rust `ExportPreferences` struct, snake_case included. */
export interface ExportPreferences {
  /** PNG pixel density, 1 to 3. */
  scale: number;
  transparent: boolean;
  /** Store the drawing inside exported images, so they open back in Excalidraw. */
  embed_scene: boolean;
}

export const loadExportPreferences = () => invoke<ExportPreferences>("load_export_preferences");
export const saveExportPreferences = (preferences: ExportPreferences) =>
  invoke<void>("save_export_preferences", { preferences });
