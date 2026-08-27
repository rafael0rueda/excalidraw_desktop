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
