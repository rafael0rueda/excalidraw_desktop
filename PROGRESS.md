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

## Useful commands

```bash
npm start                      # tauri dev
npx tsc --noEmit               # typecheck
cargo build --manifest-path src-tauri/Cargo.toml
npm run bundle                 # RPM
```
