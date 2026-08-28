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
| 4 | Autosave + session restore | **Done** |
| 5 | Theme engine + presets | **Done** |
| 6 | Theme editor UI | **Built, not yet checked by eye** |
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

## Theming findings — the basis of phase 5

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

## Theme engine (phase 5, built)

**Schema.** Ten colours, deliberately hand-authorable — anything requiring a
look-up in Excalidraw's stylesheet is derived instead of asked for:

```jsonc
{ "id": "my-theme", "name": "My Theme", "dark": true,
  "colors": {
    "canvas": "#1F1F28", "surface": "#2A2A37", "surfaceAlt": "#363646",
    "text": "#DCD7BA", "textMuted": "#727169",
    "accent": "#7E9CD8", "accentText": "#1F1F28", "danger": "#E82424",
    "stroke": "#DCD7BA", "fill": "transparent" } }
```

`src/theme/variables.ts` expands that into 64 CSS custom properties. It sets
**base** variables wherever it can, because Excalidraw derives many properties
with `var()` (`--text-primary-color: var(--color-on-surface)`,
`--popup-bg-color: var(--island-bg-color)`, `--button-hover-bg:
var(--color-surface-high)`), and those resolve against the computed value on the
element — so overriding the base carries the whole chain.

**Shipped presets:** Light, Dark, Nord, Dracula, Gruvbox Dark, Solarized
Light/Dark, Catppuccin Mocha, Kanagawa Wave, Kanagawa Lotus. Kanagawa hexes come
straight from `~/.local/share/nvim/lazy/kanagawa.nvim/lua/kanagawa/colors.lua`
with surfaces mapped as that theme maps its own (`bg` / `bg_p1` / `bg_p2`) —
not eyeballed. Wave/Lotus is the pair this machine's desktop already uses.

**User themes:** `~/.config/excalidraw-desktop/themes/*.json`, read at startup
and on View → Theme → Reload user themes. A user theme reusing a preset's `id`
replaces it in place. Invalid files are skipped with a named reason on reload,
and silently at startup — a broken file should not greet the user with a dialog.

**Settings:** `~/.config/excalidraw-desktop/settings.json` —
`{ theme, light_theme, dark_theme }`. `theme` is a theme id or `"system"`;
`"system"` reads GNOME's `color-scheme` via `gsettings` and picks from the pair.
The pair is editable from the editor panel (phase 6) as well as in the file.
Re-checked on window focus, so a desktop-wide light/dark flip follows.

### Two things Excalidraw 0.18.1 will not let us theme

Both verified by reading the shipped bundle, not guessed:

0. **The swatch row cannot be themed, and inverting it is a trap.** The web
   app's dark mode shows light grey for the *same* stored `#1e1e1e`, via
   `--theme-filter: invert(93%) hue-rotate(180deg)`. That variable is settable
   and is applied to `.color-picker__button`, `.color-picker-swatch`,
   `.color-picker-label-swatch` and the eye-dropper — while the canvas filter is
   gated on the `.excalidraw.theme--dark canvas` *selector*, not the variable.
   So setting it would invert the swatches without inverting the canvas: every
   swatch would then display the opposite of what it draws. We keep it `none`
   and default to a genuinely light stroke instead, so what you see is what is
   stored. The cost is that the five fixed top picks stay literal — the first
   one is black.
1. **Grid colour.** Hardcoded as `{Bold:"#dddddd", Regular:"#e5e5e5"}` in a
   module-private object in `chunk-K2UTITRG.js`, read at draw time. Not in
   `appState` (which has only `gridSize`, `gridStep`, `gridModeEnabled`) and not
   a CSS variable. Unreachable, so the schema deliberately has no `grid` key.
2. **The colour-picker palette.** `UIOptions` exposes only
   `dockedSidebarBreakpoint`, `canvasActions`, `tools.image` and a deprecated
   `welcomeScreen`. No palette hook, so the schema has no `palette` key either.

Dead configuration is worse than a missing feature; if a later Excalidraw
exposes either, add the key then.

### Behaviour decisions

- The theme owns the canvas background and the default element colours, and
  they are merged into the *same* `updateScene` call that replaces a scene
  (`ThemedDefaults` in `document.ts`). Doing it afterwards does not work:
  Excalidraw commits a replaced scene on its own render pass, which lands after
  ours, so a repaint scheduled from an effect is silently overwritten. This was
  observed, not theorised — an element drawn 28 s after a restart still came out
  `#1e1e1e`.
- `currentItemStrokeColor` is flagged `export: false`, so it is never stored in
  a `.excalidraw` file; `loadFromBlob` fills it with Excalidraw's own `#1e1e1e`.
  Every scene load therefore has to re-assert the theme's stroke, or dark themes
  draw invisible black.
- **`api.getAppState()` does not reflect your own `updateScene` yet.** Probed
  directly: a mark taken immediately after `updateScene` still reported the old
  `#1e1e1e` / `#ffffff`, while a sample 100 ms later reported the theme's
  values. Any read-then-write of appState that spans a render boundary loses.
  Verified by writing samples to `/tmp` from the running app — the only way to
  observe this, since the webview console is not forwarded to `tauri dev`.
- Switching theme retints the *default* stroke/fill for new elements only while
  they still match the outgoing theme. A colour the user picked is theirs.
- Existing elements are never recoloured.

## Theme editor (phase 6, built)

`components/ThemeEditor.tsx` — a fixed side panel opened from **View → Theme →
Customise themes…** or `Ctrl+,`. Edits the ten colours with a live preview,
and holds the appearance controls (what the app uses, and the follow-system
pair, which previously only existed in `settings.json`).

Design decisions worth keeping:

- **The draft is painted through the controller, not written to disk.**
  `useTheme` gained `preview(theme | null)`; a draft outranks the chosen theme
  for as long as the panel is open and never reaches `settings.json`. Dropping
  the preview on unmount is what makes "close without saving" work.
- **Half-typed colours are repaired before painting** (`draft.paintable`).
  `color.ts` passes unparseable values straight through — that is deliberate, so
  `"transparent"` survives `mix` — which means `#12` would otherwise land in a
  CSS variable mid-keystroke.
- **Ids are validated in Rust, not only in the renderer.** An id arrives from
  the renderer and becomes a path, so `settings::theme_file` accepts only
  `[a-z0-9-]{1,64}`; `../` and absolute paths are rejected outright rather than
  merely being unlikely. Covered by a unit test.
- **Changing the appearance reloads the editor onto the result; saving does
  not.** Otherwise the panel and the screen could disagree about which theme is
  in play. The `adopt` ref is what distinguishes the two cases.
- **The renderer serialises the file, Rust just writes it.**
  `save_user_theme` takes the finished text plus an id, parses it only to check
  it, and writes the caller's bytes. Round-tripping through
  `serde_json::Value` sorted the keys alphabetically, which scatters colours
  that belong together and puts the whole `colors` block above the `id` naming
  it. `types.serializeTheme` emits schema order; `parseTheme` rebuilds `colors`
  from `THEME_COLOR_KEYS`, so a hand-written file in any order is normalised
  the first time it is saved from the editor. Enabling `serde_json`'s
  `preserve_order` would have been one line, but Cargo unifies features, so it
  would switch every map in the build — Tauri's included — to `IndexMap` for a
  cosmetic gain.
- **Save keeps the id, Save as new derives one from the name.** Saving over a
  preset id is a supported way to customise a built-in theme, since `merge()`
  already lets a user file replace a preset in place; the panel says so.
- The panel styles itself from the ten theme colours via its own `--ed-*`
  variables. It cannot use Excalidraw's: those are set inline on the
  `.excalidraw` element (see `apply.ts`) and do not reach a sibling.
- Keystrokes are stopped at the panel root, and the window-level accelerator
  listener in `App.tsx` skips events originating inside it. Excalidraw binds
  single-key tool shortcuts on the document.

`npm run check` (`scripts/check-theme.mjs`) covers the pure parts: preset
validity, that preset ids satisfy the Rust id rule, `parseTheme` diagnostics,
well-formed CSS output for every preset, and the id/repair helpers. It uses
esbuild, which Vite already depends on, rather than adding a test runner.

**Not yet verified by eye.** The app cannot be driven in an ordinary browser —
`App.tsx` calls `getCurrentWindow()` on mount, so React never renders outside
Tauri — which rules out headless checking of this panel. It needs a human
looking at the window.

## Next steps

1. Look at the theme editor in a running window: open it, drag a colour, check
   the panel and the app repaint together, save as new, reopen, delete.
2. Verify by hand what is still unproven: open, save, export, clipboard, and the
   clean-quit path (quit normally, relaunch, expect *no* recovery prompt).
   Crash recovery itself is already verified.
3. Phase 7 (tabs) or phase 8 (RPM + .desktop + MIME association).

## Autosave & session restore (phase 4)

State lives in `~/.config/excalidraw-desktop/session/`: `scene.excalidraw` (the
snapshot, under its real extension so a failed recovery still leaves a file the
user can open by hand) and `meta.json` (`path`, `dirty`, `saved_at`,
`clean_exit`).

Design decisions worth keeping:

- **Autosave never writes to the user's file.** It only refreshes the snapshot.
  Silently rewriting a drawing the user has not saved is a worse failure than
  losing a few seconds of work.
- **`clean_exit` is what distinguishes recovery from convenience.** It is
  `false` in every snapshot and set to `true` by `mark_clean_exit`, called from
  `endSession()` just before the window is destroyed. So:
  - `!clean_exit && dirty` → crash or kill; offer to restore the snapshot.
  - otherwise → reopen `meta.path` from disk, quietly (it may have been deleted).
  This is why work the user explicitly *discarded* on close does not come back.
- **Snapshots are suppressed until startup has decided what to restore**
  (`restored` ref in `document.ts`) — otherwise the empty initial canvas would
  overwrite the snapshot we are about to read.
- **Timing:** 1.5 s debounce after the last edit, with a 10 s ceiling so a
  snapshot never slips further behind while the user keeps drawing.
- Restoring a snapshot sets `savedVersion` to `NEVER_SAVED` (-1), so the
  document stays dirty until the user actually saves it.
- The startup effect is guarded by a ref, not the usual `cancelled` flag —
  StrictMode double-invokes effects and the recovery dialog must not appear
  twice.

To force the recovery prompt by hand: draw something without saving, then
`kill -9` the app (a clean quit deliberately will not trigger it).

**Verified by hand on 2026-08-27:** drew two elements without saving, `kill -9`,
relaunched — the recovery dialog appeared and restoring brought the scene back
intact, still marked dirty.

`cargo test --lib` covers the snapshot file lifecycle.

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

## Useful commands

```bash
npm start                      # tauri dev
npx tsc --noEmit               # typecheck
npm run check                  # theme module assertions
cargo test --lib --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
npm run bundle                 # RPM
```
