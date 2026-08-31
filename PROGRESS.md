# Project state & how to resume

Last updated: 2026-08-30

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
| 6 | Theme editor UI | **Done** |
| 7 | Tabs / multiple drawings | **Built, not yet checked by eye** |
| 8 | RPM + .desktop + MIME association | **Done** (installed and launched on this machine) |

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

### The GTK chrome (added 2026-08-29)

The menu bar and the menus that drop out of it are GTK widgets, not part of the
page, so they kept the desktop's widget theme — a white strip above a dark
canvas. `src-tauri/src/chrome.rs` paints them with a `gtk::CssProvider` on the
default screen at `STYLE_PROVIDER_PRIORITY_APPLICATION` (600), which outranks
the widget theme's own provider (200). `applyTheme` calls it, so a preview in
the editor carries the menu bar with it.

- The provider is held in a **thread-local** on the GTK main thread, not in
  Tauri state: GTK types are `!Send`, and keeping it is what lets an update
  reload the same provider instead of stacking another one on the screen.
- Colours are **validated in Rust** before they reach the stylesheet
  (`css_color`): a theme is a local file the user wrote, but a value out of a
  file should not be able to write CSS rules.
- `:backdrop` is spelled out, or the widget theme greys the bar out whenever the
  window loses focus.
- The same rules target `menu`, which is also what WebKit puts up for a
  `<select>` — so the theme editor's dropdowns should follow too.

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

## Tabs (phase 7, built)

`lib/tabs.ts` is the model, `lib/document.ts` the controller, `components/
TabBar.tsx` the strip along the top. React state holds only what the bar and the
menu draw (`{id, path, dirty}` per tab); the scenes live in a ref-held `Map` so
drawing does not re-render the shell.

Design decisions worth keeping:

- **An inactive tab is `.excalidraw` text, not a live scene.** Excalidraw is one
  editor instance, so a switch is a scene load — the same path as open, save and
  snapshot, which already works. The alternative, holding several live scenes and
  swapping element arrays, means holding objects Excalidraw mutates in place.
- **Two things that text does not carry are kept beside it.** The viewport
  (`scrollX`/`scrollY`/`zoom` are flagged `export: false`, so a saved file has no
  viewport) and the scene version the tab was last saved at, which is what the
  dirty flag compares against.
- **A capture is refused while `committed` is false.** Excalidraw commits a
  replaced scene on its own render pass, which lands after ours (see the phase-5
  note on `getAppState`), so in that window `getSceneElements()` still reports the
  *outgoing* drawing. Capturing then would file one tab's scene under another
  tab's id — the one bug in this design that would silently destroy work. The
  flag is cleared when we hand Excalidraw a scene and set again in `onChange`.
- **Undo history belongs to the instance, not to a drawing.** So a switch calls
  `api.history.clear()`, and scene loads pass
  `captureUpdate: CaptureUpdateAction.NEVER`. Otherwise `Ctrl+Z` would rewind
  past the drawing now on screen, into another tab's edits.
- **Saved versions of inactive tabs are resolved lazily** (`UNPARSED`). Working
  one out means parsing the tab's text, which at startup would mean parsing every
  restored drawing at once; instead it is done the first time a tab is shown.
- **Autosave sends a scene only for tabs that have changed.** Each tab carries a
  `rev` that moves only when its text does, and `save_session` treats an omitted
  scene as "keep the file you have". Without it, drawing in one tab would rewrite
  every open tab's snapshot every 1.5 s.
- **Tab ids are validated in Rust**, by the same `store::safe_id` as theme ids —
  they arrive from the renderer and become file names.
- **A pre-tabs session is adopted rather than dropped.** `load_session` falls back
  to the legacy `path`/`dirty` fields and `scene.excalidraw`, under the tab id
  `restored`; the old file is pruned on the next save.
- **The tab bar is shown even with one tab open.** On GNOME/Wayland the titlebar
  never updates, so this row is the only place the filename and the unsaved
  marker actually appear.
- **Reopening a session does not touch the recent-files list.** Those files were
  added when they were opened; pushing all of them on every launch would order
  the list by tab position rather than by when the user last reached for
  something. (This is a deliberate change from the single-document version, which
  pushed the one restored file.)

`--ed-*` became `--ui-*` and moved to `theme/panel.ts`, since the tab bar and the
theme editor now style themselves from the same set.

**Not yet verified by eye**, for the same reason as the theme editor: `App.tsx`
calls `getCurrentWindow()` on mount, so React never renders outside Tauri and the
window cannot be driven headlessly. What *was* verified from outside: the app
starts, writes a `meta.json` in the new shape with one tab, and its snapshot
carries the theme's canvas colour — so startup, restore and autosave all ran.

## Next steps

All eight phases are built. The RPM is **installed on this machine** (0.4.0):
the app appears in the GNOME app grid with its icon, launches from there, and
the GTK menu bar and its drop-down menus come up in the theme's colours — so
the chrome work of phase 7 is confirmed on a real desktop, not just under
`tauri dev`. What is left is verification by a human at the machine, and then
whatever the app turns out to want in use.

1. Double-click a `.excalidraw` file in Files: it should open in the app, with
   the app's icon on it. Then select two and open them together, and
   double-click a third while the app is running — one window, three tabs.
   (Launching from the app grid works; the *file association* is the untested
   half.)
2. Look at the tab bar in a running window: open two files, switch, check each
   tab keeps its own viewport and dirty dot, close a dirty tab, close the last
   tab (should leave an empty one, not quit), then `kill -9` with two dirty tabs
   and check the recovery prompt offers both back.
3. Verify by hand what is still unproven: open, save, export, clipboard, and the
   clean-quit path (quit normally, relaunch, expect *no* recovery prompt).
   Crash recovery itself is already verified, but only for a single drawing.
4. Still open from the theme chrome: whether the theme editor's `<select>`
   pop-ups follow the theme. They are WebKit `menu` widgets and the same CSS
   provider targets them, but only the menu bar has actually been seen.

Already verified by hand, so don't re-do it: **the theme editor saves a working
user theme.** Dracula was edited in the panel (`Ctrl+,`) and saved; that wrote
`~/.config/excalidraw-desktop/themes/dracula.json`, and because a saved theme
keeps the preset's `id`, it replaced the shipped Dracula in place — the View →
Theme menu listed the edited name instead of a second entry, which is the
intended behaviour. The test file has since been removed and the preset is back.

## Packaging (phase 8, built)

`npm run bundle` produces **both** an `.rpm` and a `.deb` under
`src-tauri/target/release/bundle/` — package name `excalidraw-desktop`, ~17 MB
each. Everything they install beyond the binary lives in `packaging/`, and the
two formats share the desktop template and the maintainer scripts, because
nothing in them is format-specific.

Neither bundler shells out to `rpmbuild` or `dpkg-deb` — both are Rust, and
neither name appears in the Tauri CLI binary — so this Fedora machine builds the
`.deb` as readily as the `.rpm`. Only the `.rpm` has ever been installed.

What a working file association actually needs, in order:

1. **A MIME type must exist.** `packaging/excalidraw-desktop.xml` goes to
   `/usr/share/mime/packages/` and defines
   `application/vnd.excalidraw+json` — Excalidraw's own type string. Without it
   there is nothing for the desktop entry's `MimeType=` to name, and a `.excalidraw`
   file is seen as plain JSON.
2. **The desktop entry must accept a file argument.** This is the one thing
   Tauri gets wrong: its built-in template writes `Exec=excalidraw-desktop` with
   no field code, so the file manager launches the app and the drawing is never
   passed. `packaging/excalidraw-desktop.desktop.hbs` (wired up as
   `desktopTemplate` under both `bundle.linux.rpm` and `bundle.linux.deb`) is a
   copy of Tauri's output plus
   `Exec={{exec}} %F`, `Keywords`, and `Version=1.0`. The template variables the
   bundler actually provides were established by probing: `name`, `comment`,
   `categories`, `exec`, `icon` and `mime_type` render; `exec_arg`,
   `file_associations` and `identifier` come out empty.
3. **The caches must be rebuilt.** `packaging/post-install.sh` and
   `post-remove.sh` — the same two files for both formats — run
   `update-mime-database`, `update-desktop-database` and `gtk-update-icon-cache`.
   Fedora runs all three from rpm file triggers and Debian from dpkg triggers,
   so on those they are belt and braces; they are what makes the packages work
   on distributions that ship neither.

Decisions worth keeping:

- **Dependencies are declared as sonames, not package names.** The rpm asks for
  `libwebkit2gtk-4.1.so.0()(64bit)` and `libgtk-3.so.0()(64bit)` and nothing
  else: `bundle.linux.rpm.depends` was *removed*, because naming Fedora's
  `webkit2gtk4.1` and `gtk3` would have made the package refuse to install on
  openSUSE or Mageia, where those packages are called something different. The
  soname requires are generated automatically and every RPM distribution
  resolves them — `dnf repoquery --whatprovides` maps both straight back to
  `webkit2gtk4.1` and `gtk3` here, so Fedora loses nothing.
- **`bundle.linux.deb.depends` was removed for a different reason.** Tauri
  appends its own `libwebkit2gtk-4.1-0, libgtk-3-0` to whatever is listed, so
  spelling them out produced a `Depends:` line with each name twice. Letting
  Tauri supply them alone gives a clean one.
- **The deb puts the licence at `/usr/share/doc/excalidraw-desktop/copyright`**,
  not `/usr/share/licenses/`, which is where Debian policy wants it. Same file,
  different `files` mapping per format.
- **No AppImage.** `npx tauri build --bundles appimage` gets as far as
  `linuxdeploy`'s GTK plugin and fails there: the plugin wants `librsvg-2.0.pc`
  (`librsvg2-devel`, not installed here), and on Fedora 44 linuxdeploy's own
  bundled `strip` is too old for the `.relr.dyn` sections in current system
  libraries, so it also needs `NO_STRIP=1`. Both are fixable; nothing has been
  verified, so the README does not offer an AppImage.
- **`sub-class-of application/json` stays.** It is accurate, and the worry that
  it would let a JSON-handling app outrank us proved wrong: with our entry
  installed, `gio mime application/vnd.excalidraw+json` reports us as both
  default and the only *recommended* app, while text editors remain available
  under "Open With".
- **The magic block sniffs `"type": "excalidraw"` including the closing quote.**
  Without the quote it would also match `"type": "excalidrawlib"`, which is a
  library file and not a drawing. Priority 40, below the glob's default 50.
- **One instance, not many** (`tauri-plugin-single-instance`). Double-clicking a
  second drawing used to be one command away from starting a second app against
  the same config directory — and `save_session` prunes snapshots that are not in
  the tab list, so the two copies would have deleted each other's unsaved work.
  The second launch now forwards its argv over D-Bus (local IPC, no network) and
  exits; the running instance opens the files as tabs and asks to be raised.
- **Relative paths are resolved against the *caller's* cwd**, which the plugin
  hands over with the arguments. The running instance may sit in a different
  directory entirely. Paths are then canonicalised, so the same drawing named
  two ways lands in one tab.
- **The command line no longer replaces the session, it adds to it.** Opening
  only the named file would have left every other tab out of the next snapshot,
  and the snapshot is pruned to what is open — so double-clicking a drawing
  would have quietly discarded the rest of the session. Startup now restores the
  session first and opens the command-line drawings on top, as extra tabs.
- **`128x128@2x.png` is not in `bundle.icon`.** Tauri maps it to
  `/usr/share/icons/hicolor/256x256@2/`, which is not a directory any icon theme
  reads. The 32, 128 and 512 icons cover what is used.
- **MIT**, matching Excalidraw's own licence, so the app and the library it
  wraps say the same thing. `bundle.license` sets the rpm `License` tag;
  `Cargo.toml` and `package.json` carry the identifier too, since each is the
  canonical place for its own ecosystem. `bundle.licenseFile` is set but the rpm
  bundler ignores it — verified by `rpm -qlp`, which showed no licence file —
  so the text is delivered through `bundle.linux.rpm.files` instead, to
  `/usr/share/licenses/excalidraw-desktop/LICENSE` where Fedora expects it. It
  is an ordinary file rather than a `%license` one, which only matters to
  `rpm --excludedocs`.

Verified without installing anything, by extracting the package and pointing
`XDG_DATA_HOME` at a scratch directory:

- `desktop-file-validate` passes; `Exec=excalidraw-desktop %F` and
  `MimeType=application/vnd.excalidraw+json` are both present.
- `gio info` resolves `drawing.excalidraw` to the type by glob, an
  extensionless copy by magic, and a `excalidrawlib` file to `text/plain`.
- `gio mime` names our entry the default handler (with a stub binary on `PATH`,
  which GIO requires before it will consider a desktop entry at all).

And by running the release binary against a scratch `XDG_CONFIG_HOME`:

- launching it with a drawing opens that drawing;
- a second launch with a *relative* path exits immediately (rc 0, no second
  window) and the first window gains a second tab holding the resolved path;
- killing it and relaunching with a third drawing yields three tabs — the two
  restored plus the new one, which is the one focused.

Still unverified: the installed package on the real system, i.e. double-clicking
a file in Files and seeing the app's icon on it.

## Autosave & session restore (phase 4)

State lives in `~/.config/excalidraw-desktop/session/`: one `<tab-id>.excalidraw`
per open tab (under its real extension so a failed recovery still leaves a file
the user can open by hand) and `meta.json` (`tabs[]` of `{id, path, dirty}`,
`active`, `saved_at`, `clean_exit`). Phase 7 widened this from a single
`scene.excalidraw`; see "Tabs" for what that changed.

Design decisions worth keeping:

- **Autosave never writes to the user's file.** It only refreshes the snapshot.
  Silently rewriting a drawing the user has not saved is a worse failure than
  losing a few seconds of work.
- **`clean_exit` is what distinguishes recovery from convenience.** It is
  `false` in every snapshot and set to `true` by `mark_clean_exit`, called from
  `endSession()` just before the window is destroyed. So:
  - `!clean_exit` and any tab dirty → crash or kill; offer to restore the
    snapshots, all of them, in one prompt.
  - otherwise → reopen each tab's `path` from disk, quietly, dropping the ones
    whose file has gone (it may well have been deleted).
  This is why work the user explicitly *discarded* on close does not come back.
- **Snapshots are suppressed until startup has decided what to restore**
  (`restored` ref in `document.ts`) — otherwise the empty initial canvas would
  overwrite the snapshot we are about to read.
- **Timing:** 1.5 s debounce after the last edit, with a 10 s ceiling so a
  snapshot never slips further behind while the user keeps drawing.
- Restoring a snapshot sets `savedVersion` to `NEVER_SAVED` (-1), so the
  document stays dirty until the user actually saves it. A tab that was *clean*
  gets `UNPARSED` (-2) instead, which resolves to its real version when shown.
- The startup effect is guarded by a ref, not the usual `cancelled` flag —
  StrictMode double-invokes effects and the recovery dialog must not appear
  twice.

To force the recovery prompt by hand: draw something without saving, then
`kill -9` the app (a clean quit deliberately will not trigger it).

**Verified by hand on 2026-08-27** (single-drawing version; the multi-tab
prompt has not been through this yet): drew two elements without saving, `kill -9`,
relaunched — the recovery dialog appeared and restoring brought the scene back
intact, still marked dirty.

`cargo test --lib` covers the snapshot file lifecycle.

## Code review, verified against a running instance (2026-08-31)

Twelve review findings were checked empirically, not just read. GUI input
injection is impossible on this box (GNOME/Wayland refuses XTEST to Xwayland
clients), so the frontend ones were driven through a throwaway Tauri-IPC
harness: the real, unmodified renderer running in a browser against a fake
`window.__TAURI_INTERNALS__.invoke` over an in-memory filesystem. The harness
files were deleted afterwards; rebuild them if these need re-testing.

Two harness lessons worth keeping:

- `_unlisten` calls `window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener`
  *before* invoking `plugin:event|unlisten`. A fake IPC layer that omits that
  global makes every unlisten throw, and `void pending.then(...)` swallows it —
  which looks exactly like a listener leak that is not there.
- `confirm()` from `@tauri-apps/plugin-dialog` invokes `plugin:dialog|message`
  and compares the result to `okLabel`. The backend returns the *button label*,
  not a boolean, so a fake that answers `true` silently takes the cancel path.

Confirmed, in severity order:

1. **Save race loses work.** `writeTo` (`src/lib/document.ts`) recomputes
   `savedVersion` from the live scene *after* awaiting the disk write. With a
   1.2 s write latency, an element drawn during the write ended up
   `sceneHasIt:true, diskHasIt:false, tabShowsDirty:false`; quitting asked
   nothing, marked a clean exit, and the next launch came back without it.
2. **Export selection drops bound text and frame children.** `sceneFor` in
   `src/lib/exports.ts` filters on raw `selectedElementIds`, which by Excalidraw's
   design excludes bound labels. Select-all then export selection: 4230 bytes
   full vs 1049 selection-only, label absent.
3. **`saveAsNew` overwrites its source.** `taken.delete(draft.id)` means an
   unedited name re-derives the *same* id; the "new" theme wrote `light.json`.
   Renaming first gave the expected `light-copy`.
4. **Unsaved-changes prompt has no Cancel.** Buttons are
   `{"OkCancelCustom":["Save","Discard"]}`; Escape resolves to the cancel label
   and is treated as Discard, closing a dirty tab with zero writes.
5. **`endSession` marks a clean exit even when the snapshot is suppressed.**
   Quitting while `load_session` is still pending gave `saveSessionCalls:0,
   markCleanExitCalls:1` on a session whose `clean_exit` was `false`. Next
   launch: no recovery prompt, unsaved work gone.
6. **`writeTo` is the only mutation site that does not also write
   `tabsRef.current`.** Save-then-quit wrote the file but snapshotted
   `{path:null, dirty:true}`. Worse, the re-render schedules a *late* snapshot
   that lands after `mark_clean_exit` and flips `clean_exit` back to `false` —
   observed directly. Whichever side wins the race against `window.destroy()`,
   the next launch is wrong: a saved file reopened as an untitled dirty tab, or
   a spurious recovery prompt.
7. **Non-canonical dialog paths open one file twice.** `cli_drawings()`
   canonicalises; the dialog path does not. Opening a file and then a symlink to
   it went from 1 tab to 2.
8. **`adopt.current` stays armed.** Changing the *dark* pair while the desktop
   is light leaves `theme.chosen` identity unchanged, so the effect never
   consumes the flag. Editing a colour and then letting the desktop flip to dark
   replaced the unsaved draft with no discard prompt. Control run without the
   arming step kept the draft — so the flag, not the flip, is the cause.
9. **`write_atomic` drops permissions and breaks links.** Reproduced with the
   exact function: a `600` target came back `644`; a symlink was replaced by a
   regular file leaving the target untouched; a hard link was broken (link count
   1, the other name kept the old content).
10. **Invalid theme JSON is unreportable.** `list_user_themes` drops it with
    `.ok()`, so the renderer's `errors[]` path in `readUserThemes` — which exists
    precisely to report rejects — can only ever see files that already parsed.
11. **`"csp": null`** in `tauri.conf.json` alongside unrestricted
    `read_text_file`/`write_text_file`.

Downgraded — do not fix what is not broken:

- The `onCloseRequested` effect depending on the unstable `actions` object was
  reported as leaving a gap where a close is not intercepted, plus a listener
  leak. **Neither happens.** Measured sequence `LLULLUULULLUU...` with
  `minAfterStart:1`: the new listener always registers before the old one
  unlistens, and listens/unlistens balanced 8/8. Only the per-render IPC churn
  is real, and that is a performance nit.

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
npm run check                  # theme + tab module assertions
cargo test --lib --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
npm run bundle                 # RPM

# inspecting the package without installing it
R="src-tauri/target/release/bundle/rpm/Excalidraw Desktop-0.4.0-1.x86_64.rpm"
rpm -qip "$R"; rpm -qlp "$R"; rpm -qp --scripts "$R"
rpm2cpio "$R" | (cd /tmp && cpio -idm)   # then read /tmp/usr/share/applications/*.desktop
```
