# Project state & how to resume

Last updated: 2026-08-27

## Decisions already made (do not re-litigate)

- **Tauri v2, not Electron.** Chosen after empirically testing Excalidraw 0.18.1
  inside WebKitGTK 2.52.5 (the engine Tauri embeds). See "Engine findings".
- **Packaging: RPM** via `tauri build`, plus `.desktop` entry and `.excalidraw`
  MIME association (phase 8).
- **Theming: full editor + presets** (phases 5–6), themes as JSON in
  `~/.config/excalidraw-desktop/themes/`.
- Excalidraw pinned to **0.18.1**. Do not bump without re-running the engine test.

## Phase status

| # | Phase | Status |
|---|---|---|
| 1 | Foundation — Tauri+React+Excalidraw, offline fonts, window state | **Done** |
| 2 | Files — open/save/save-as, recent, dirty guard, CLI arg | **Done** |
| 3 | Export — PNG, SVG, clipboard | **Done** |
| 4 | Autosave + session restore | Not started |
| 5 | Theme engine + presets | Not started |
| 6 | Theme editor UI | Not started |
| 7 | Tabs / multiple drawings | Not started |
| 8 | RPM + .desktop + MIME association | Config stubbed, not built |

## Engine findings (verified by experiment, keep these)

Tested by rendering a real Excalidraw bundle in WebKitGTK 2.52.5 via the
`webkit2gtk-4.1` GObject typelib and screenshotting the result.

1. **WebKitGTK renders Excalidraw correctly.** Fonts (incl. CJK), rough.js
   strokes, hachure/cross-hatch fills, dark UI chrome, PNG export (34 KB blob),
   SVG export with embedded `@font-face`. 20k stroked béziers in 4 ms.
2. **`showOpenFilePicker` is absent** in WebKitGTK (present in Chromium).
   Irrelevant — we use native GTK dialogs via `tauri-plugin-dialog`.
3. **`navigator.clipboard.write()` is gated behind user activation** in
   WebKitGTK. This is why `clipboard.rs` copies images through the **GTK
   clipboard on the main thread**, not the web API.

## Theming findings — READ BEFORE STARTING PHASE 5

Both were verified by experiment and both apply to Electron too; they are
Excalidraw quirks, not engine quirks.

1. **Custom dark themes must use `theme="light"` as the base.**
   Excalidraw's dark mode applies an *invert filter* to the canvas. Setting
   `theme="dark"` with a dark `viewBackgroundColor` (`#2e3440`) produced a
   **light grey-blue canvas** — the filter inverted it. Using `theme="light"`
   with genuinely dark colours gives a correct dark theme with no inversion.

2. **A `<style>` sheet cannot override Excalidraw's CSS custom properties.**
   `:root, .excalidraw { --island-bg-color: ... }` loses on specificity; the
   computed value stayed Excalidraw's own `#232329`. **Fix:** set the custom
   properties as *inline styles with `!important`* on the `.excalidraw` root
   element:

   ```ts
   const root = document.querySelector(".excalidraw") as HTMLElement;
   root.style.setProperty("--island-bg-color", "#3b4252", "important");
   ```

   Verified working — computed value read back `#3b4252` and a full Nord theme
   rendered correctly.

The variables that matter: `--color-primary`, `--color-primary-darker`,
`--color-primary-darkest`, `--color-primary-light`, `--island-bg-color`,
`--default-bg-color`, `--text-primary-color`, `--icon-fill-color`,
`--button-hover-bg`, `--default-border-color`, `--popup-bg-color`,
`--popup-text-color`, `--sidebar-bg-color`, `--sidebar-border-color`,
`--input-bg-color`, `--input-border-color`, `--color-on-surface`,
`--color-surface-low`, `--color-surface-mid`, `--color-surface-high`,
`--color-surface-primary-container`, `--select-highlight-color`,
`--focus-highlight-color`.

## Planned theme JSON schema (phase 5)

```jsonc
{ "id": "nord", "name": "Nord", "base": "light",   // always "light" for dark themes
  "ui":     { "accent": "#88c0d0", "surface": "#3b4252", "text": "#eceff4", ... },
  "canvas": { "background": "#2e3440", "grid": "#434c5e", "defaultStroke": "#d8dee9" },
  "palette": ["#bf616a", "#d08770", "#ebcb8b", "#a3be8c", "#b48ead"] }
```

Presets to ship: Light, Dark, Nord, Dracula, Gruvbox, Solarized light/dark,
Catppuccin Mocha. Plus a "follow GNOME" option via `prefers-color-scheme`.

## Next steps

1. `npm start` and verify phases 1–3 by hand (open, save, export, clipboard).
2. Phase 4: autosave to `~/.config/excalidraw-desktop/session/`, restore on launch.
3. Phase 5: theme engine, applying the two findings above.

## Gotchas

- **The debug binary is not standalone.** `cargo build` produces a binary that
  loads `devUrl` (`http://localhost:1420`); running it without `npm start`
  shows "Could not connect to localhost: Connection refused". Only the release
  build embeds `dist/`. Always launch dev via `npm start`.

- **GTK calls must run on the GTK main thread.** `clipboard.rs` uses
  `app_handle().run_on_main_thread(...)` and resolves the GTK handle *inside*
  the closure (GTK types are `!Send`). Assume other native integrations need it.

## Parked: window title on GNOME/Wayland

The window title does not update at runtime. The titlebar permanently shows the
value from `tauri.conf.json`; `Untitled — Excalidraw Desktop` and the `•` dirty
marker never appear. **Cosmetic only — deliberately parked, do not re-derive.**

What was established, with evidence:

1. The command runs and returns `Ok`. Confirmed by stderr instrumentation.
2. `tao`'s `set_title` returns before applying — an immediate read-back returns
   the *old* title, the next call reads the new one. So `Ok` is not evidence the
   change landed.
3. Setting the title on the GTK window from the GTK main thread **does** apply:
   `gtk_window.title()` reads back the new value on a window reporting
   `realized=true visible=true`.
4. There is exactly **one** toplevel — enumerating `gtk::Window::list_toplevels()`
   showed a single `GtkApplicationWindow`, `visible=true decorated=true`, holding
   the correct new title. So it is not a wrong-handle or second-window problem.
5. The decoration *does* honour the creation-time title: setting
   `tauri.conf.json` to `CONFIG-TITLE-TEST` displayed exactly that, while GTK
   simultaneously held `Untitled — Excalidraw Desktop`.
6. An explicit `gdk::Display::flush()` after `set_title` did not help.

Conclusion: GTK holds the right title; Mutter does not pick up the runtime
change. Not an application-logic bug.

Ruled out along the way: stale/duplicate app instances (verified single PID),
IPC failure, permissions, the em-dash, and a second GTK window.

If ever revisited: check whether forcing `GDK_BACKEND=x11` makes updates
propagate (would confirm a Wayland-specific path, at the cost of native
Wayland), or retest after a Tauri/tao upgrade. Note that GNOME denies both
`org.gnome.Shell.Eval` and `org.gnome.Shell.Introspect.GetWindows` to
unsandboxed callers, so the title cannot be read back from the compositor —
verification needs a human looking at the titlebar.

## Useful commands## Useful commands

```bash
npm start                      # tauri dev
npx tsc --noEmit               # typecheck
cargo build --manifest-path src-tauri/Cargo.toml
npm run bundle                 # RPM
```
