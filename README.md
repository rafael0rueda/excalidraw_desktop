# Excalidraw Desktop

A fully local, themeable Excalidraw drawing app for Linux. No web app, no
account, no network access at runtime.

Built with **Tauri v2** (Rust + system WebKitGTK) rather than Electron: the
resulting app is ~20 MB instead of ~200 MB, and `tauri build` emits native
`.rpm` and `.deb` packages directly.

## Which distributions this runs on

The app needs exactly two things at runtime: **WebKitGTK 4.1**
(`libwebkit2gtk-4.1.so.0`) and **GTK 3** (`libgtk-3.so.0`). WebKitGTK 4.1 is the
deciding one — it is the GTK 3 build of WebKit against libsoup 3, and it is what
dates the list below. Anything older ships only the 4.0 ABI and will not do.

| Distribution | Status | Install with |
|---|---|---|
| Fedora 38+ | Tested — developed and run on Fedora 44 | the `.rpm` |
| RHEL 9+ / AlmaLinux / Rocky / CentOS Stream 9+ | Should work, not tried | the `.rpm` |
| openSUSE Leap 15.5+ / Tumbleweed | Should work, not tried | the `.rpm` |
| Mageia 9+ | Should work, not tried | the `.rpm` |
| Debian 12 (bookworm)+ | Should work, not tried | the `.deb` |
| Ubuntu 22.04+ | Should work, not tried | the `.deb` |
| Linux Mint 21+, Pop!\_OS 22.04+, elementary 7+ | Should work, not tried | the `.deb` |
| Arch, Manjaro, EndeavourOS | No package — [build from source](#building-from-source) | — |
| Void, Gentoo, NixOS, Alpine, anything else | No package — [build from source](#building-from-source) | — |

"Should work, not tried" is meant literally: the packages declare the right
dependencies and contain the right files, and only Fedora has actually had one
installed. Both packages are `x86_64`/`amd64` only. There is no ARM build,
though nothing in the source prevents one — `npm run bundle` on an ARM machine
should produce it.

The RPM deliberately requires the two **shared libraries** rather than package
names. Fedora calls them `webkit2gtk4.1` and `gtk3`, openSUSE and Mageia call
them something else, and a soname is the one thing they all agree on — `dnf`,
`zypper` and `urpmi` each resolve it to whatever their own package is.

## Installing

The packages are not published anywhere; build them first with `npm run bundle`
(see [Building from source](#building-from-source)), which writes both into
`src-tauri/target/release/bundle/`. The quotes in the commands below matter —
the file names contain a space.

### Fedora, RHEL, AlmaLinux, Rocky, CentOS Stream

```bash
sudo dnf install "src-tauri/target/release/bundle/rpm/Excalidraw Desktop-0.4.0-1.x86_64.rpm"
sudo dnf remove excalidraw-desktop     # to uninstall
```

### openSUSE

```bash
sudo zypper install --allow-unsigned-rpm "src-tauri/target/release/bundle/rpm/Excalidraw Desktop-0.4.0-1.x86_64.rpm"
sudo zypper remove excalidraw-desktop
```

### Debian, Ubuntu, Mint, Pop!\_OS, elementary

```bash
sudo apt install "./src-tauri/target/release/bundle/deb/Excalidraw Desktop_0.4.0_amd64.deb"
sudo apt remove excalidraw-desktop
```

The leading `./` is required — without a path separator `apt` looks for a
package by that name in the repositories instead. The deb depends on
`libwebkit2gtk-4.1-0` and `libgtk-3-0`; on Ubuntu 24.04 and Debian 13 the latter
is supplied by `libgtk-3-0t64`, which provides the old name, so `apt` still
resolves it. If it ever reports the dependency as unsatisfiable:

```bash
sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0t64
sudo dpkg -i --force-depends "./src-tauri/target/release/bundle/deb/Excalidraw Desktop_0.4.0_amd64.deb"
```

### Arch, and everything else

There is no package. Install the two runtime libraries with your package manager
(on Arch: `sudo pacman -S webkit2gtk-4.1 gtk3`) and then either run the built
binary in place —

```bash
./src-tauri/target/release/excalidraw-desktop
```

— or install it by hand, which is all the packages do anyway:

```bash
stage="src-tauri/target/release/bundle/rpm/Excalidraw Desktop-0.4.0-1.x86_64"

sudo install -Dm755 src-tauri/target/release/excalidraw-desktop /usr/bin/excalidraw-desktop
sudo install -Dm644 packaging/excalidraw-desktop.xml \
                    /usr/share/mime/packages/excalidraw-desktop.xml
sudo install -Dm644 "$stage/usr/share/applications/Excalidraw Desktop.desktop" \
                    "/usr/share/applications/Excalidraw Desktop.desktop"

# The 512 px icon is icons/icon.png; the other two are named after their size.
sudo install -Dm644 src-tauri/icons/32x32.png   /usr/share/icons/hicolor/32x32/apps/excalidraw-desktop.png
sudo install -Dm644 src-tauri/icons/128x128.png /usr/share/icons/hicolor/128x128/apps/excalidraw-desktop.png
sudo install -Dm644 src-tauri/icons/icon.png    /usr/share/icons/hicolor/512x512/apps/excalidraw-desktop.png

sudo update-mime-database /usr/share/mime
sudo update-desktop-database /usr/share/applications
sudo gtk-update-icon-cache -qtf /usr/share/icons/hicolor
```

The last three lines are what the packages' `%post` / `postinst` runs; without
them the desktop will not know about the new file type until the next login.
`npm run bundle` has to have run at least once as well — the desktop entry is
generated during bundling, not checked in.

What lands on the system either way:

| Path | What it is |
|---|---|
| `/usr/bin/excalidraw-desktop` | The app |
| `/usr/share/applications/Excalidraw Desktop.desktop` | Menu entry and file handler |
| `/usr/share/mime/packages/excalidraw-desktop.xml` | Teaches the desktop what a `.excalidraw` file is |
| `/usr/share/icons/hicolor/*/apps/excalidraw-desktop.png` | Icons (32, 128, 512) |
| `/usr/share/licenses/excalidraw-desktop/LICENSE` (rpm) | The licence text |
| `/usr/share/doc/excalidraw-desktop/copyright` (deb) | The same text, where Debian policy wants it |

## Building from source

Build dependencies: **Node 20+ with npm**, and a Rust toolchain plus the GTK and
WebKitGTK development headers.

| Distribution | Command |
|---|---|
| Fedora, RHEL, Alma, Rocky | `sudo dnf install rust cargo nodejs webkit2gtk4.1-devel libsoup3-devel openssl-devel gtk3-devel` |
| Debian, Ubuntu, Mint | `sudo apt install rustc cargo nodejs npm libwebkit2gtk-4.1-dev libsoup-3.0-dev libssl-dev libgtk-3-dev build-essential` |
| openSUSE | `sudo zypper install rust cargo nodejs npm && sudo zypper install -C 'pkgconfig(webkit2gtk-4.1)' 'pkgconfig(gtk+-3.0)' 'pkgconfig(libsoup-3.0)' 'pkgconfig(openssl)'` |
| Arch, Manjaro | `sudo pacman -S rust nodejs npm webkit2gtk-4.1 libsoup3 openssl gtk3 base-devel` |

Then:

```bash
npm install
npm run bundle     # → src-tauri/target/release/bundle/{rpm,deb}/
```

`npm run bundle` builds **both** package formats on any distribution. Neither
bundler shells out to `rpmbuild` or `dpkg-deb` — they are written in Rust, and
neither name appears anywhere in the Tauri CLI binary — so a Fedora machine can
produce the `.deb` and a Debian machine the `.rpm`, and `rpm-build` is not a
build dependency. That is how the `.deb` here was made; it has never been
installed.

An AppImage is possible in principle (`npx tauri build --bundles appimage`) but
is not built here: Tauri's AppImage step runs `linuxdeploy`'s GTK plugin, which
needs `librsvg-2.0.pc` — `sudo dnf install librsvg2-devel` on Fedora — and on
Fedora 44 its bundled `strip` is also too old for the `.relr.dyn` sections in
current system libraries, so it needs `NO_STRIP=1`. Untested, so it is not
listed above.

## Running from source

```bash
npm install
npm start          # tauri dev — launches the real desktop window
```

> **Do not launch `src-tauri/target/debug/excalidraw-desktop` directly.** A debug
> build points at `devUrl` (`http://localhost:1420`) and does *not* embed the
> frontend, so on its own it shows "Could not connect to localhost: Connection
> refused". Only release builds (`npm run bundle`) embed `dist/`. Use `npm start`
> for development.

## Opening drawings from the file manager

Installing the package registers `application/vnd.excalidraw+json` and makes
this app its default handler, so double-clicking a `.excalidraw` file opens it.
Files are recognised by their extension, and also by their contents if the
extension is missing.

Selecting several drawings and opening them together puts each in its own tab of
a single window — **the app only ever runs once.** A second launch hands its
files to the window already open instead of starting a rival copy, which matters
because two copies would share one config directory and prune each other's
session snapshots.

Drawings named on the command line (`excalidraw-desktop a.excalidraw b.excalidraw`)
open the same way, as extra tabs on top of the session being restored.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+T`, `Ctrl+N` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+Tab`, `Ctrl+PageDown` | Next tab |
| `Ctrl+Shift+Tab`, `Ctrl+PageUp` | Previous tab |
| `Ctrl+1` … `Ctrl+9` | Go to tab |
| `Ctrl+O` | Open |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save As |
| `Ctrl+Shift+P` | Export PNG |
| `Ctrl+Shift+G` | Export SVG |
| `Ctrl+Shift+C` | Copy image to clipboard |
| `Ctrl+,` | Customise themes |

Shortcuts are registered both as native menu accelerators and as a
window-level capture listener, because Excalidraw's canvas swallows many
keystrokes on its own.

## Tabs

Several drawings can be open at once, one per tab. The bar along the top is
also where the filename and the unsaved-changes dot live, since the titlebar
does not update on GNOME/Wayland (see *Known limitations*).

- **Open** reuses the current tab when it is empty and unsaved, and otherwise
  opens a new one. A file that is already open is brought forward rather than
  opened twice.
- **Close** asks about unsaved changes, showing you the drawing it is asking
  about. Closing the last tab leaves an empty one — quitting is `Ctrl+Q`.
- Each tab keeps its own viewport, so switching back puts you where you were.
- Undo history is shared by the editor, not by the drawing, so it is cleared on
  a switch: `Ctrl+Z` can never reach into another tab.
- Every open tab is snapshotted, so a crash offers all of them back at once.

## Theming

A theme covers the whole window: the canvas and Excalidraw's panels, the tab
bar, and the menu bar. That last one is a GTK widget rather than part of the
page, so it is painted separately by the backend (`src-tauri/src/chrome.rs`)
through a GTK style provider.

**View → Theme** switches between the built-in themes: Light, Dark, Nord,
Dracula, Gruvbox Dark, Solarized Light/Dark, Catppuccin Mocha, **Kanagawa Wave**
and **Kanagawa Lotus**. *Follow system* tracks GNOME's light/dark preference and
picks from a pair (Kanagawa Lotus / Kanagawa Wave by default).

### The theme editor

**View → Theme → Customise themes…** (`Ctrl+,`) opens a side panel. Pick any
theme to start from, change its ten colours, and watch the app repaint as you
type — nothing is written until you press a save button.

- **Save** writes the theme under its current id. Doing this to a built-in
  theme means your version replaces it from then on; delete the file to get the
  original back.
- **Save as new** derives a fresh id from the name and leaves the original
  alone.
- **Delete** removes a theme you saved.

The same panel holds the appearance controls: what the app uses, and which two
themes *Follow system* switches between.

### Your own themes

The editor writes ordinary JSON files — in the order below, so they stay
readable — and you can equally write one yourself. Drop it in
`~/.config/excalidraw-desktop/themes/`:

```json
{
  "id": "my-theme",
  "name": "My Theme",
  "dark": true,
  "colors": {
    "canvas": "#1F1F28",
    "surface": "#2A2A37",
    "surfaceAlt": "#363646",
    "text": "#DCD7BA",
    "textMuted": "#727169",
    "accent": "#7E9CD8",
    "accentText": "#1F1F28",
    "danger": "#E82424",
    "stroke": "#DCD7BA",
    "fill": "transparent"
  }
}
```

Ten colours, expanded into Excalidraw's ~64 CSS variables for you. Use **View →
Theme → Reload user themes** after editing; files that do not parse are listed
with the reason. Reusing a built-in `id` overrides that theme.

Preferences live in `~/.config/excalidraw-desktop/settings.json`
(`theme`, `light_theme`, `dark_theme`), but there is nothing in there the
editor panel does not also expose.

## Autosave and recovery

Your work is snapshotted to `~/.config/excalidraw-desktop/session/` a second or
two after you stop drawing. **Autosave never writes to your own `.excalidraw`
file** — saving stays something you ask for.

- If the app crashes or is killed with unsaved changes, the next launch offers
  to restore them — every tab, not just the one you were looking at.
- If you quit normally, the next launch simply reopens the drawings you had
  open. Changes you chose to discard on the way out stay discarded.

## Known limitations

- **The grid colour is not themeable.** Excalidraw 0.18.1 hardcodes it
  (`#dddddd` / `#e5e5e5`) in its renderer, with no `appState` or CSS hook.
- **The colour-picker palette is not themeable** either — `UIOptions` exposes no
  hook for it. A theme sets the *default* stroke and fill for new elements, but
  the swatch grid stays Excalidraw's own.
- **A second launch may not raise the window on GNOME/Wayland.** The drawing
  does open as a new tab, but the compositor is entitled to refuse the
  focus request and merely highlight the app in the dock instead.
- **The window title does not update on GNOME/Wayland.** The titlebar always
  shows `Excalidraw Desktop`; the current filename and unsaved-changes marker do
  not appear there. Investigated and parked — see PROGRESS.md. Cosmetic only.

## Architecture

```
src/                    Renderer — React 19 + Excalidraw. No filesystem access.
  lib/api.ts            Typed wrappers over the Rust command layer
  lib/document.ts       Tabs, open/save, dirty tracking, autosave (useDocument hook)
  lib/exports.ts        PNG/SVG/clipboard rendering
  lib/exportActions.ts  Dialog-driven export flows
  lib/menu.ts           Native menu construction
  lib/scene.ts          Excalidraw (de)serialisation helpers
  lib/tabs.ts           The tab model and its pure helpers
  theme/types.ts        Theme schema and validation
  theme/presets.ts      Built-in themes
  theme/variables.ts    Theme -> Excalidraw CSS custom properties
  theme/apply.ts        Paints a theme onto the running instance
  theme/useTheme.ts     Theme selection, persistence, follow-system, live preview
  theme/draft.ts        Pure helpers behind the editor (ids, colour repair)
  theme/panel.ts        The --ui-* variables our own chrome styles itself from
  components/TabBar.tsx       The open drawings
  components/ThemeEditor.tsx  The theme editor panel
src-tauri/src/          Rust backend
  files.rs              Atomic file reads/writes
  recent.rs             Recent-files list in ~/.config/excalidraw-desktop/
  session.rs            Autosave snapshots (one per tab) + crash recovery
  settings.rs           Preferences, user themes, system colour scheme
  store.rs              Config-directory location and the id-to-filename rule
  clipboard.rs          GTK-native clipboard image copy
  chrome.rs             Paints the GTK menu bar and menus in the theme's colours
scripts/copy-assets.mjs Copies Excalidraw fonts into public/ for offline use
scripts/check.mjs       Assertions over the pure modules (`npm run check`)
packaging/              What the packages install beyond the binary
  excalidraw-desktop.xml           MIME definition for .excalidraw
  excalidraw-desktop.desktop.hbs   Desktop entry template (adds the Exec field code)
  post-install.sh                  Refreshes the MIME/desktop/icon caches
  post-remove.sh                   ...and again once the definition is gone
```

**Offline guarantee.** Excalidraw fetches its handwriting fonts (Excalifont,
Nunito, Comic Shanns) from a CDN by default. `scripts/copy-assets.mjs` copies
them out of the npm package into `public/`, and `index.html` sets
`window.EXCALIDRAW_ASSET_PATH = "./"` before Excalidraw loads.

**Config location.** `~/.config/excalidraw-desktop/` — currently `recent.json`;
window geometry is persisted by `tauri-plugin-window-state`.

## Licence

MIT — see [LICENSE](LICENSE). The RPM carries the same identifier and installs
the text to `/usr/share/licenses/excalidraw-desktop/`.

The app bundles [Excalidraw](https://github.com/excalidraw/excalidraw) (MIT) and
the font files that ship inside it, which keep their own upstream licences.
