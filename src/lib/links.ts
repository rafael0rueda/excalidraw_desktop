import { isElementLink } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Follows a link on an element.
 *
 * Excalidraw would open it inside this webview, and `lib.rs` refuses any
 * navigation away from the app — the page would otherwise replace the app,
 * unsaved work and close guard with it. So a link to part of this drawing
 * scrolls there, and anything else goes to the desktop's own browser or mail
 * client; the opener's scope lets only http, https, mailto and tel through.
 */
export function openLink(api: ExcalidrawImperativeAPI, link: string): void {
  if (isElementLink(link)) {
    const params = new URL(link, window.location.href).searchParams;
    const id = params.get("element") ?? params.get("group");
    const targets = api
      .getSceneElements()
      .filter((el) => id !== null && (el.id === id || el.groupIds.includes(id)));
    if (targets.length) api.scrollToContent(targets, { fitToContent: true, animate: true });
    return;
  }
  openUrl(link).catch(() => {});
}
