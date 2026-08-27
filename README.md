# Excalidraw Desktop

A fully local, themeable Excalidraw drawing app for Fedora Linux. No web app, no
account, no network access at runtime.

Built with **Tauri v2** (Rust + system WebKitGTK) rather than Electron: the
resulting app is ~20 MB instead of ~200 MB, and `tauri build` emits a native
`.rpm` directly.

## Requirements

Fedora 42+ with:

```bash
sudo dnf install rust cargo webkit2gtk4.1-devel libsoup3-devel openssl-devel gtk3-devel rpm-build
```

Node 20+ and npm.

## Running

```bash
npm install
npm start          # tauri dev — launches the real desktop window
```

> **Do not launch `src-tauri/target/debug/excalidraw-desktop` directly.** A debug
> build points at `devUrl` (`http://localhost:1420`) and does *not* embed the
> frontend, so on its own it shows "Could not connect to localhost: Connection
> refused". Only release builds (`npm run bundle`) embed `dist/`. Use `npm start`
> for development.

## Building an RPM

```bash
npm run bundle     # → src-tauri/target/release/bundle/rpm/*.rpm
sudo dnf install src-tauri/target/release/bundle/rpm/*.rpm
```

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+N` | New drawing |
| `Ctrl+O` | Open |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save As |
| `Ctrl+Shift+P` | Export PNG |
| `Ctrl+Shift+G` | Export SVG |
| `Ctrl+Shift+C` | Copy image to clipboard |

Shortcuts are registered both as native menu accelerators and as a
window-level capture listener, because Excalidraw's canvas swallows many
keystrokes on its own.

## Known limitations

- **The window title does not update on GNOME/Wayland.** The titlebar always
  shows `Excalidraw Desktop`; the current filename and unsaved-changes marker do
  not appear there. Investigated and parked — see PROGRESS.md. Cosmetic only.

## Architecture

```
src/                    Renderer — React 19 + Excalidraw. No filesystem access.
  lib/api.ts            Typed wrappers over the Rust command layer
  lib/document.ts       Open/save/dirty-tracking controller (useDocument hook)
  lib/exports.ts        PNG/SVG/clipboard rendering
  lib/exportActions.ts  Dialog-driven export flows
  lib/menu.ts           Native menu construction
  lib/scene.ts          Excalidraw (de)serialisation helpers
src-tauri/src/          Rust backend
  files.rs              Atomic file reads/writes
  recent.rs             Recent-files list in ~/.config/excalidraw-desktop/
  clipboard.rs          GTK-native clipboard image copy
scripts/copy-assets.mjs Copies Excalidraw fonts into public/ for offline use
```

**Offline guarantee.** Excalidraw fetches its handwriting fonts (Excalifont,
Nunito, Comic Shanns) from a CDN by default. `scripts/copy-assets.mjs` copies
them out of the npm package into `public/`, and `index.html` sets
`window.EXCALIDRAW_ASSET_PATH = "./"` before Excalidraw loads.

**Config location.** `~/.config/excalidraw-desktop/` — currently `recent.json`;
window geometry is persisted by `tauri-plugin-window-state`.
