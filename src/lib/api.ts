// Typed wrappers around the Rust command layer. The renderer never touches the
// filesystem directly — everything goes through these invocations.
import { invoke } from "@tauri-apps/api/core";

export interface RecentEntry {
  path: string;
  name: string;
  opened_at: number;
}

export const readTextFile = (path: string) => invoke<string>("read_text_file", { path });

export const writeTextFile = (path: string, contents: string) =>
  invoke<void>("write_text_file", { path, contents });

/** `data` is base64 — JSON number arrays are far too slow for multi-MB PNGs. */
export const writeBinaryFile = (path: string, data: string) =>
  invoke<void>("write_binary_file", { path, data });

export const copyImageToClipboard = (data: string) =>
  invoke<void>("copy_image_to_clipboard", { data });

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
export const clearSession = () => invoke<void>("clear_session");

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
/** Raw JSON from ~/.config/excalidraw-desktop/themes; the caller validates. */
export const listUserThemes = () => invoke<unknown[]>("list_user_themes");
export const systemColorScheme = () => invoke<"light" | "dark">("system_color_scheme");
/**
 * Writes `<id>.json` into the themes directory; resolves to the file written.
 * The caller supplies the finished text so that its key order survives — see
 * `serializeTheme`.
 */
export const saveUserTheme = (id: string, contents: string) =>
  invoke<string>("save_user_theme", { id, contents });
export const deleteUserTheme = (id: string) => invoke<void>("delete_user_theme", { id });

/** A path passed on the command line, e.g. from a file-manager double click. */
export const startupFile = () => invoke<string | null>("startup_file");

export const setWindowTitle = (title: string) => invoke<void>("set_window_title", { title });

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000; // avoid blowing the argument limit on large buffers
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export async function blobToBase64(blob: Blob): Promise<string> {
  return bytesToBase64(new Uint8Array(await blob.arrayBuffer()));
}
