export {};

declare global {
  interface Window {
    EXCALIDRAW_ASSET_PATH: string;
  }
}

// Must run before Excalidraw's own module evaluates, so its fonts resolve
// locally rather than from a CDN. Its own file and script tag, loaded before
// main.tsx's, rather than an inline <script> in index.html: an inline script
// has no `src` for the CSP's script-src to allow without 'unsafe-inline',
// while this is just another same-origin asset.
window.EXCALIDRAW_ASSET_PATH = "./";
