# Iced port — plan

Branch: `iced-port` (from `main` at 75225c2). Written 2026-11-11.
Status: phases 0–8 done; the phase 8 review found the editor short of
parity (see the end of this file).

## Goal

Rebuild Excalidraw Desktop as a native Rust app on **Iced 0.14**, with no web
engine, and keep files fully compatible with Excalidraw (excalidraw.com and the
current Tauri build must open what the Iced build saves, and the reverse).

The Tauri app stays on `main` and keeps shipping. The Iced app replaces it only
if it passes the go/no-go gate (phase 1) and then reaches parity (phase 8).

## Why this is a rewrite, not a swap

The current editor is `@excalidraw/excalidraw` 0.18.1, a React library. Iced
cannot run JavaScript, so everything Excalidraw does must be rebuilt in Rust:
the scene model, hand-drawn rendering, tools, selection, text, arrows, undo,
export. What carries over is the ~2.7k lines of Rust in `src-tauri/src` that
are already almost free of Tauri (`store.rs`, `scope.rs` have no Tauri
references at all; `session.rs`, `settings.rs`, `recent.rs`, `files.rs` only use
it at the command boundary), plus `packaging/` and the theme JSON format.

## Verified versions (crates.io, 2026-11-11)

| Crate | Version | Use |
|---|---|---|
| `iced` | 0.14.0 | UI. Default features include `wgpu` **and** `tiny-skia` (software fallback) and `linux-theme-detection` |
| `roughr` | 0.14.0 | Port of rough.js — hand-drawn strokes and hachure fills |
| `kurbo` | 0.13.1 | Bézier geometry, hit-testing, bounding boxes |
| `tiny-skia` | 0.12.0 | Offscreen PNG export |
| `usvg` / `resvg` | 0.49.0 | Render embedded SVG images; check exported SVG |
| `cosmic-text` | 0.19.0 | Text shaping and layout (iced already uses it) |
| `rfd` | 0.18.0 | File dialogs through the XDG desktop portal |
| `arboard` | 3.7.0 | Clipboard (text + images) |
| `vello` | 0.11.0 | Only if the canvas needs a faster renderer (see W1) |

Toolchain here: rustc 1.99.1.

## Workspace layout

```
iced/                      Cargo workspace (new)
  crates/core/             config dir, atomic writes, session, settings,
                           recent, themes — moved out of src-tauri, no UI deps
  crates/scene/            Excalidraw data model, serde, geometry, hit-testing,
                           undo/redo — no UI deps, #![forbid(unsafe_code)]
  crates/render/           scene → drawing commands (roughr), shared by the
                           iced canvas, the SVG exporter and the PNG exporter
  crates/app/              the Iced application
src-tauri/                 unchanged except it depends on crates/core
```

The Tauri app also uses `crates/core`, so both builds read the same session,
settings and themes on disk and can be compared on the same data. The
`core` extraction is useful even if the port is abandoned and could go to
`main` on its own.

## Phases

Each phase ends with a commit and an update to this file's status table.

| # | Phase | Size | Exit criteria |
|---|---|---|---|
| 0 | Workspace + extract `core` | S | Tauri app builds and passes `scripts/check.mjs` using `crates/core`; `cargo test` green |
| 1 | **Spike: read-only viewer** | M | Opens real `.excalidraw` files; draws rectangle, ellipse, diamond, line, arrow, freedraw, text, image; pan + zoom. **Go/no-go gate below** |
| 2 | Editing core | L | Create/select/move/resize/rotate/delete, multi-select, group, z-order, undo/redo, copy/paste in Excalidraw's clipboard JSON |
| 3 | Text and arrows | L | On-canvas text editing with IME, text bound in containers, arrows bound to shapes, arrowheads, labels |
| 4 | Files, tabs, session | M | Open/save/save-as, atomic writes, recent, dirty guard, tabs, autosave + crash recovery, CLI args, single instance |
| 5 | Themes | M | Ten presets, theme editor, user themes JSON, follow-system light/dark |
| 6 | Export + library | M | PNG, SVG, copy-as-PNG; library panel with the ER and Kanagawa ERD sets; `.excalidrawlib` import |
| 7 | Packaging | S | `.rpm` + `.deb`, `.desktop`, MIME association, reusing `packaging/` |
| 8 | Parity review | S | Feature checklist vs README, benchmark table, decision: replace, keep both, or stop |

### Phase 1 go/no-go gate

Measure the Iced viewer against the Tauri build on the same machine and the
same files: a small drawing, a 2k-element and a 10k-element scene.

| Metric | Continue if Iced is… |
|---|---|
| Pan/zoom frame time (10k scene) | at least 2× better |
| Resident memory after load | at most half |
| Cold start to first frame | faster |
| Visual fidelity | side-by-side screenshots look like the same drawing |

If the gate fails, stop here; the cost of phases 2–8 is only worth it for a
clear win. Record numbers in this file either way.

### Gate result, 2026-11-11: **GO**

Measured on this machine (Fedora, Wayland, 180 Hz display, radv Vulkan),
release builds. Reference: Excalidraw 0.18.1 itself, in WebKitGTK 2.54.2
(the engine the Tauri app embeds), driven by `iced/bench/webkit.py` through
the same animation: 5 s panning a 300 px circle, 5 s zooming 1×→4×→1×.
Reproduce: `iced/bench/run.sh` and `iced/bench/memory.sh` (header comments
say how to build their inputs).

| Scene | Metric | iced | Excalidraw (WebKitGTK) | |
|---|---|---|---|---|
| 10k | pan, mean frame / p95 | 8.1 / 9.6 ms (123 fps) | 35.5 / 41 ms (28 fps) | **4.4×** |
| 10k | zoom, mean frame / p95 | 11.9 / 13.5 ms (84 fps) | 26.4 / 82 ms (38 fps) | **2.2×** |
| 10k | CPU per frame, all processes | 5.6 ms | 34.7 ms | 6× |
| 2k | pan / zoom | 179 / 173 fps (vsync) | 59 / 43 fps | |
| 2k | CPU per frame | 1.3 ms | 19.9 ms | 15× |
| small | CPU per frame | 0.4 ms | 3.2 ms | 8× |
| 10k | memory, PSS of all processes | 170 MB | 778 MB (Tauri app) | **22%** |
| 2k | memory | 63 MB | 484 MB (Tauri app) | 13% |
| small | memory | 45 MB | 273 MB (Tauri app) | 16% |
| 10k / 2k / small | start to drawing | 235 / 90 / 59 ms | 507 / 420 / 376 ms¹ | **2–6×** |

¹ WebView creation to "scene loaded", before its first paint and without
the Tauri shell around it, so the real gap is larger.

WebKitGTK caps `requestAnimationFrame` at 60 fps whatever the display, so
for the smaller scenes CPU per frame is the comparison that means anything.

**Fidelity.** Side by side at the same view (`--screenshot` and
`webkit.py shot`), every stroke is the same stroke: rough.js and
perfect-freehand are ported bit for bit, and Excalidraw's shape code is
checked against Excalidraw itself (`render/tests/excalidraw_shapes.rs`, 153
elements). Differences found, none of them about shape:

- **Thin lines are heavier.** iced_wgpu's 4× MSAA gives sub-pixel strokes
  about 45% more ink (hachure coverage 18.6% vs 12.7% in Excalidraw;
  iced's own tiny-skia renderer gives 13.6%, aliased wgpu 12.4%, so the
  geometry is right). Hachure fills look denser. → W1: analytic AA, by
  rendering the scene with vello in a `shader` widget, or by fixing MSAA.
- **Frame outline and name** are drawn into the cached geometry, so they
  scale between zoom buckets where Excalidraw keeps them a fixed screen
  size. → draw frame chrome in an uncached layer (phase 2).
- **Fonts split by Unicode range** (Excalifont, Nunito, Lilita One, Comic
  Shanns) only have their Basic Latin file bundled: cosmic-text does not
  fall back across files of one family. Other scripts use system fonts.
  CJK (Xiaolai, 12 MB) is not bundled. → merge the files at build time
  (phase 3).

**Found on the way, and settled:**

- iced's canvas cache cannot be re-transformed, so a pan through `Canvas`
  re-tessellates everything. The viewer is a small custom widget that draws
  the cached meshes under `renderer.with_transformation`, rebuilding only
  when the zoom leaves its power-of-two bucket (W1).
- iced's tessellator moves path points by the frame transform but not
  stroke widths or dash lengths. Strokes are mapped to cache space by hand.
- Re-tessellating the 10k scene takes 72 ms, the zoom benchmark's worst
  frames (125 ms). → tessellate per element, off the UI thread (phase 2).

### v1 scope

| In v1 | Later | Out |
|---|---|---|
| Rectangle, ellipse, diamond, line, arrow, freedraw, text, image | Elbow arrows, frames | Live collaboration, share links |
| Stroke/fill styles, roughness, opacity, fonts | Eraser, laser pointer | Mermaid → Excalidraw (dropped, decision 1) |
| Bound text, bound arrows | Lasso selection | Embedded web pages (already blocked today) |
| Everything in the README "What this adds" list | Search in canvas | |

## Iced's weak points and how this plan addresses them

**W1. Raw rendering is below GPUI.**
- Three canvas `Cache` layers: the static scene (redrawn only when
  elements change), the selection overlay, and the stroke being drawn.
  Panning and zooming change a transform and do not re-tessellate.
- Pre-compute each element's roughr output once per change, keyed by
  `(id, version)`. Excalidraw files already carry `version` per element.
- Spatial index (R-tree) for hit-testing and for drawing only what is
  visible.
- Escape route: if profiling in phase 1 shows the canvas tessellator is the
  bottleneck, render the scene through a custom `shader` widget with
  `vello`. Its `wgpu` version must match iced's; check before adopting.

**W2. Text editing is less mature than a browser's.**
- Edit on-canvas text with an iced `text_editor` placed over the element,
  the same trick Excalidraw uses with a `<textarea>`.
- Test IME (fcitx5/ibus with Japanese and Chinese input), dead keys and
  AZERTY in phase 3. Treat a failure as a blocker, not polish.
- Bundle Excalidraw's fonts (Excalifont, Nunito, Lilita One, Comic Shanns,
  Liberation Sans for Helvetica, Cascadia/Source Code) and fall back to
  system fonts for CJK through `cosmic-text`/fontdb.

**W3. Pre-1.0 API churn.**
- Pin `iced = "=0.14.0"`.
- Keep all logic in `core`, `scene` and `render`, which do not depend on
  iced. An upgrade then only touches `crates/app`.

**W4. No native menu bar.**
- Draw the menu bar in-app with Iced widgets. It follows the theme with no
  extra work, which removes the GTK recolouring code in `chrome.rs`.
- Keyboard shortcuts move into one table shared by the menus and the key
  handler. Keep the layout-independent fallback described in the README.

**W5. Fewer ready-made widgets** (tabs, colour picker, menus).
- Write the few needed (tab bar, colour picker, dropdown menu) in
  `crates/app/widgets`, or use `iced_aw` if it has a 0.14-compatible
  release at the time.

**W6. Accessibility is limited in Iced.**
- The canvas is not accessible in the web build either, so the loss is the
  panels and dialogs.
- Make every action reachable by keyboard and check focus order in each
  phase. Follow Iced's accessibility work and adopt it when it ships.

**W7. Excalidraw compatibility and upstream drift.** This is the biggest risk.
- Every element struct carries
  `#[serde(flatten)] extra: serde_json::Map<String, Value>`, so unknown or
  newer fields survive a load/save round-trip untouched.
- Golden tests: load and re-save a fixture corpus (files made by
  excalidraw.com 0.18 and by the current Tauri build). The JSON must compare
  equal, apart from fields we deliberately change.
- Use each element's `seed` with roughr. Check in phase 1 whether
  roughr reproduces rough.js's shapes for the same seed. If not, drawings
  still open correctly but the wobble differs; decide if that is acceptable.

## Security: what improves and what stays

Removed attack surface:
- No JavaScript engine, no HTML/CSS parser, no webview IPC bridge. The 26
  Tauri commands and the capability file go away.
- File paths come straight from the native dialog or the command line into
  Rust, with no untrusted renderer in between. `scope.rs` becomes
  unnecessary in the Iced build.
- No WebKitGTK dependency, so its security updates no longer matter.

Kept or added:
- **Untrusted input is the drawing file.** Limit file size before parsing,
  element count, point count per freedraw, and image size.
- Decode images with explicit size limits (the `image` crate's `Limits`) and
  render embedded SVG with `usvg`, which does not fetch external resources.
- Keep atomic writes and the private config dir from `store.rs` as they are.
- Links open in the system browser only for `http`, `https` and `mailto`.
- `#![forbid(unsafe_code)]` in `core`, `scene` and `render`.
- `cargo-deny` and `cargo-audit` in CI.
- Fuzz the scene parser with `cargo-fuzz`, seeded from the golden corpus.

## Side benefits for packaging

- Runtime dependencies shrink to glibc, libxkbcommon, Wayland/X11 libs and
  Vulkan or GL, which is optional because of the tiny-skia fallback. WebKitGTK
  4.1 and GTK 3 are no longer needed.
- Without the WebKitGTK version constraint, the app can be built in an older
  container (glibc 2.31–2.35) and run on Debian 12 and Ubuntu 22.04. Today only
  Fedora 44 is supported.
- `linux-theme-detection` (iced default) replaces the GTK/portal code in
  `settings.rs` for follow-system dark mode. File dialogs use the XDG portal
  through `rfd`, so GTK can be dropped completely.

## Decisions (2026-11-11, from Rafa)

1. **Mermaid import is dropped.** Not in v1, not planned later.
2. **The Iced build does not ship until all feature work is done** (phase 8).
   Until then it is not packaged for users, and `main` keeps releasing the
   Tauri app.
3. **Drawings: identical wobble preferred; compatible required.** Matching
   rough.js shape-for-shape (same seed → same strokes) is the goal. The floor
   is that every drawing made by Excalidraw or the Tauri build opens and
   round-trips without loss, even if strokes wobble differently.

## Open questions

1. ~~Does roughr match rough.js for the same seed?~~ **Answered 2026-11-11:
   no, and it cannot**: roughr draws from `StdRng` (ChaCha), rough.js from a
   Park–Miller generator. Replaced by `iced/crates/rough`, a port of rough.js
   4.6.4 that matches it **bit for bit** on 343 Excalidraw-shaped cases
   (`tests/rough_js.rs`). What that took:
   - The same RNG, shared between option copies the way `o.randomizer` is.
   - Every expression in the same order, so rounding is the same.
   - `Math.round` (halves go up), and hachure's in-place rotate-and-back,
     whose drift a cross-hatch's second pass sees.
   - V8's own `sin`/`cos`: the original fdlibm kernels. Measured over 100k
     inputs, glibc 2.44 differed on 2.7%, the `libm` crate on 0.7%, fdlibm on
     none. `libm`'s `tan`/`atan`/`atan2`/`asin` matched on all.
   - `serde_json`'s `float_roundtrip`: without it, parsing lands one bit off.
     The scene loader needs it too.
2. Single instance without GTK: D-Bus (`zbus`) activation, or a Unix
   socket next to the existing lock file in `store.rs`?

## Status

| # | Phase | Status |
|---|---|---|
| 0 | Workspace + extract core | **Done** 2026-11-11: `iced/crates/core` (20 tests), src-tauri wrappers (6 tests), clippy clean, `npm run check` 41/41, CI runs both |
| 1 | Spike + gate | **Done** 2026-11-11, gate **GO** (results above). Crates `rough`, `scene`, `render`, `app` (viewer: `excalidraw-iced FILE`, drag or wheel to pan, Ctrl+wheel to zoom) |
| 2 | Editing core | **Done** 2026-11-11 (results below) |
| 3 | Text and arrows | **Done** 2026-11-12 (results below) |
| 4 | Files, tabs, session | **Done** 2026-11-12 (results below) |
| 5 | Themes | **Done** 2026-11-12 (results below) |
| 6 | Export + library | **Done** 2026-11-13 (results below) |
| 7 | Packaging | **Done** 2026-11-13 (results below) |
| 8 | Parity review | **Done** 2026-11-13: not at parity, decision: keep both, phases 9–11 |
| 9 | Properties panel | **Done** 2026-11-13 (results below) |
| 10 | The rest of the editor | Not started |
| 11 | Desktop parity | Not started |

### Phase 2 result, 2026-11-11

`iced/crates/editor` (no UI, 17 tests that drive it as a user would) and
the app around it: draw rectangles, diamonds, ellipses, lines, arrows and
freehand (Excalidraw's keys V R D O A L P, H to pan, Q to keep the tool);
select by click, Shift-click or rubber band, groups selected whole; move,
resize from eight handles (rotated boxes keep their fixed corner, Shift
keeps the ratio, Alt from the centre), rotate (Shift: 15°); delete, nudge,
group/ungroup, z-order, duplicate; undo/redo; copy/cut/paste through the
system clipboard in Excalidraw's own format; Ctrl+S saves.

Compatibility, checked: a drawing edited and saved here opens in Excalidraw
0.18.1 (`webkit.py shot`) with every element and stroke as drawn. Edits
follow `mutateElement` (version, versionNonce, updated), new elements carry
`_newElementBase`'s fields in order, z-order uses a port of
fractional-indexing 3.2.0, saving drops deleted elements as Excalidraw's
export does. Undo bumps versions, as Excalidraw's history does.

The scene is now tessellated in chunks of 256 elements, so an edit
re-tessellates one chunk (about 2 ms) instead of the whole drawing (72 ms at
10k), and chunks off screen are skipped: 10k pan 132 fps (was 123). The
render is pixel-identical to phase 1. `--script FILE` replays edits before
`--screenshot`, for checking the app as a whole.

Left for later phases, as planned: text and arrow labels, binding arrows to
shapes, editing a line's points (2-point lines get no box handles until
then), eraser, frames' screen-sized chrome; re-tessellating off the UI thread
(a zoom-bucket change at 10k still costs up to 41 ms).

### Phase 3, step 1: fonts (2026-11-11)

cosmic-text uses one file per family name, so Excalidraw's per-range font
files are merged at build time (`iced/crates/fontmerge`, tested on every
character of Excalifont: same outline, same advance). Xiaolai is merged into
Excalifont, which is what Excalidraw's CJK fallback amounts to. Greek,
Cyrillic, Vietnamese and CJK now render in Excalidraw's faces.

- The fonts are loaded into iced's font database borrowed from the binary:
  iced's own `font()` copied them to the heap (+40 MB for Excalifont's
  22 MB). Peak memory on the small scene: 93 MB (was 96 before merging).
- The binary grows to 55 MB, mostly Excalifont+Xiaolai uncompressed. The
  packages compress it (phase 7); storing WOFF2 and decoding at start would
  cost start-up time instead.
- Known limitation: only the Basic Latin file's kerning and mark positioning
  survive, so accents built from a base and a combining mark in another file
  (Vietnamese ế in Excalifont) sit slightly off.

### Phase 3, step 2: text (2026-11-11)

Text tool (T), double-click to type anywhere or into a shape, Enter on a
selected shape or text, Escape or Ctrl+Enter to finish. Excalidraw's text
code is ported in `editor/src/text.rs`: `measureText`, `wrapText` with its
line-break rules (whitespace, hyphens, CJK, opening and closing
punctuation), `getBoundTextMaxWidth/Height`, `computeBoundTextPosition`,
`computeContainerDimensionForBoundText`. Labels are wrapped inside their
shape, grow it when they need to, move, turn and re-wrap with it, are picked
through it, and are copied, duplicated and deleted with it. Text left empty
leaves no element and no undo step.

Widths come from shaping with the bundled fonts through iced's font system
(`app/src/measure.rs`). Checked against Excalidraw: labels typed here wrap
at the same words when the saved file is opened in Excalidraw 0.18.1.

Finding: iced's `text_editor` draws no text in `window::screenshot`
captures, with either renderer, in a stock iced app too, so what it shows
could not be checked here. So the text box draws only the caret and
selection; the element itself stays drawn underneath, laid out live as it
will be saved. Input methods go through `text_editor` as before. To verify
by hand: typing, IME (fcitx5/ibus), and caret placement on centred labels,
which can sit off the text on lines shorter than the widest.

### Phase 3, step 3: arrow labels and bindings (2026-11-12)

- Arrow labels (double-click an arrow): on its middle as
  `getBoundTextElementPosition` places them, upright, wrapped to the arrow
  label width, the stroke cut away beneath with `BOUND_TEXT_PADDING`.
- Bindings: an arrow end released inside or within `maxBindingGap` of a
  bindable element binds to it (`{ elementId, focus, gap }`, and the arrow
  in the element's `boundElements`); the shape it would bind to is
  highlighted while drawing. Moving, resizing, rotating or nudging a shape
  moves its arrows' ends (`updateBoundPoint`); an arrow moved alone re-binds
  to what its ends are on; deleting either side removes the binding
  (`fixBindingsAfterDeletion`). `determineFocusDistance`,
  `determineFocusPoint` and `updateBoundPoint` are ported as written; the
  outline intersection uses polygons (an ellipse as 64 points), within a
  fraction of a pixel.
- Checked: Excalidraw's own `restoreElements` keeps every binding of a file
  saved here (both ends, both shapes' `boundElements`, the label).
- Not yet: elbow arrows' fixed-point bindings, and dragging an arrow's end
  onto a shape after it is drawn (comes with point editing, next).

### Phase 3 result, 2026-11-12

Done: merged fonts (step 1), text and shape labels (step 2), arrow labels
and bindings (step 3), and the line editor: a handle on every point of a
selected line or arrow and in the middle of each segment; drag to move
(Shift: 15°), drag a middle one to add a point, Delete to remove the one
picked; an arrow's end dropped on a shape binds. 74 tests in the workspace.

Performance, measured against the phase 2 build side by side today (a
worktree of `c944672`): panning the 10k scene is unchanged (116 fps both;
yesterday's 132 was the machine), zooming dropped from 80 to 70 fps, worst
frame 105 ms (was 76). The likely cause is text re-rasterised at each zoom
step from the merged 42 000-glyph Excalifont; still 1.8× Excalidraw's 38 fps.
To look at with the renderer work (W1).

Still open from the phase 3 scope: elbow arrows (drawing them, and their
fixed-point bindings); verifying by hand the typing experience the
screenshots cannot show (IME, the caret on centred labels).

### Phase 4 result, 2026-11-12

`excalidraw-iced [drawing…]` is now an app rather than a viewer of one file:

- **Tabs**, each with its own drawing, view and undo history: Ctrl+N, Ctrl+W,
  Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+1…8, Ctrl+9 for the last. A "•" marks
  unsaved ones. Closing the last tab leaves an empty one.
- **Files**: Open (Ctrl+O), Save (Ctrl+S), Save as (Ctrl+Shift+S) through
  rfd's XDG portal dialogs. Writes are atomic, through core's `write_atomic`.
  Save writes back only to `.excalidraw` files; a drawing opened from
  `.json` is saved under a new name. Save as adds a missing `.excalidraw` and
  refuses a file that is open in another tab. A file already open is brought
  forward, not opened twice. A blank untitled tab is reused.
- **File menu** with the recent drawings (core's `recent`, same scope rule)
  and Clear recent.
- **Unsaved-changes guard** on closing a tab, Ctrl+Q and the window's close
  button: Save / Discard / Cancel for each unsaved tab, shown first. Escape
  is Cancel. A cancelled Save dialog cancels the quit.
- **Autosave and recovery** through core's `session`, the same files and
  rules as the Tauri app. Tabs are snapshotted 1.2 s after drawing pauses,
  never to the drawing's own file. A clean exit reopens the session as it
  was. After a crash, the app asks "N drawings were not saved… Recover?"
  first. Unreadable snapshots are moved aside, not pruned. Work discarded on
  the way out stays discarded.
- **Command line**: drawings named are opened after the session. Relative
  paths are resolved; names that are not UTF-8 are skipped with a message,
  not a panic.
- **One running copy**: core's `flock`, plus a Unix socket
  (`instance.sock`). A second launch hands its drawings to the first, which
  opens them and asks for focus, and the second launch exits with 0.
- **Its own config directory**, `~/.config/excalidraw-desktop-iced` (core's
  `set_app_dir`), so it cannot touch the Tauri app's session while both
  exist.

The dev modes (`--bench`, `--screenshot`, `--script`) still work on one
drawing and never touch the config directory. `--chrome` keeps the tab bar
and tools in screenshots.

Checked:

- An app-level test runs the whole flow through `App::update` against a
  temporary config directory:
  - open from the command line, draw, autosave;
  - a second tab, the close guard, tab keys, the File menu;
  - "crash", recover, save in place;
  - quit discarding everything, then a clean restore;
  - handover, a bad file, closing the last tab, the Save as rules.
- By hand in a real window: the handover from a second launch.

75 tests in the workspace. The chrome is plain widgets until phase 5.

### Phase 5 result, 2026-11-12

Themes work as in the Tauri app, from the same files and rules
(`app/src/theme.rs` ports `src/theme/*.ts`):

- **The ten presets.** A test holds them to `src/theme/presets.ts`.
- **User themes** are JSON in `themes/`, with the same parsing, error
  messages and `<id>.json` rule. A user theme with a preset's id takes the
  preset's place. Files are written in the schema's order, byte for byte as
  `JSON.stringify(…, null, 2)`.
- **Settings** use core's `settings.json`: `theme` (an id or `system`), plus
  `light_theme` and `dark_theme`.
- **Follow system** reads the desktop portal's `color-scheme` through
  `mundy` (already in the tree, through iced). It is read before the window
  opens and followed while the app runs.
- **What a theme paints:**
  - the canvas;
  - the colours new elements are drawn in, unless the user picked their
    own (`apply.ts`'s unclaimed rule);
  - the selection colour;
  - all the chrome: the tab bar, the File and Theme menus, the tool bar as
    a floating island, dialogs, the window's light or dark title bar.
- **Saved drawings and snapshots** carry the chosen theme's canvas colour as
  `viewBackgroundColor`, as the Tauri app writes it.
- **Theme menu:** Follow system (Light/Dark), every theme with a check on
  the one in use, Customise themes… (Ctrl+,), Reload user themes (which
  says what it skipped and why).
- **Theme editor**, a panel on the right, with the same rules as
  `ThemeEditor.tsx`:
  - Appearance (Use, and the light and dark pair);
  - Start from, Name, Dark colour scheme;
  - the ten colours as hex fields, with a red border while a value is not a
    colour;
  - Save, Save as new (a fresh id), Delete for user themes, Close.
  - The whole window paints the draft as it is edited; a half-typed colour
    paints as it was.
  - Leaving an unsaved draft asks first.
  - Saving while following the desktop fills the light or dark slot.
  - One difference: iced has no desktop colour chooser, so a swatch opens
    hue, saturation and value sliders under its field.

The development modes take `--theme ID`. Without it they keep the light
theme and each drawing's own canvas colour, for comparison with Excalidraw.

Checked:

- **Tests:**
  - unit tests for the colour grammar, `mix` and `emphasis`, the file
    format, ids, choosing a theme and the light/dark slots;
  - the app flow test extended through `App::update`: following the desktop
    as it turns dark, stroke defaults across theme changes, settings
    written, a saved file's canvas colour, the editor's draft, the discard
    question, Save as new, Delete, Reload's report.
- **Screenshots:** Light, Kanagawa Wave and Nord with the menus and the
  editor.
- **Live:** the portal read on this desktop (dark).
- **Not tried:** switching the desktop between light and dark while the app
  runs.

81 tests in the workspace. The tool bar still has text labels; icons are
part of the parity work (phase 8).

### Phase 6 result, 2026-11-13

**Export** (`crates/export`, no UI dependencies; `app/src/exporting.rs`):

- The **Export menu** as the Tauri app has it: Export PNG… (Ctrl+Shift+E),
  Export selection as PNG…, Export SVG…, Copy image to clipboard
  (Ctrl+Shift+C), and the three remembered choices in core's
  `export.json`: transparent background, embed drawing, PNG scale 1–3×.
  Exports are suggested beside the drawing and run on a thread of their own.
  "The selection" is the Tauri app's `expandSelection`: labels and what is
  inside a selected frame come with it.
- **SVG** is written from `render::prepare`, the same paint operations as
  the canvas, in `exportToSvg`'s structure:
  - `svg-source:excalidraw`, `PADDING` 10, the canvas rectangle;
  - one group per element, `translate … rotate …`;
  - frames clip their children, arrow labels are masked out of their arrows;
  - images inline, cropped and flipped;
  - links kept only for http(s) and mailto.
- **Fonts in the SVG** are inline subsets, as Excalidraw's are: only the
  characters used (`fontmerge::subset`, TrueType, licence text left out of
  the `name` table). Unlike Excalidraw's, they have no kerning: renumbering
  glyphs drops the layout tables. Chromium shows the exported SVGs with the
  right fonts.
- **PNG** is that SVG drawn by resvg 0.50 (tiny-skia, already in the tree
  through iced), with the bundled fonts only, never the system's, so a PNG
  is the same on every machine:
  - Size: `getExportSize`, truncated. Refused above 16384 px a side or
    120 Mpx.
  - Images inside a drawing can only be `data:` URLs (the href resolver
    refuses files).
  - resvg's font fallback set a whole line in Excalifont when a character
    was missing (`shape_text` replaces every glyph when the fallback covers
    them all). It now gets a subset of only the missing characters, under a
    name of its own, and falls back one character at a time, as a browser
    does.
- **The embedded drawing** is byte-compatible with Excalidraw 0.18.1:
  - zlib-deflated, as a byte string;
  - `{"version":"1","encoding":"bstring","compressed":true,…}`;
  - a `tEXt` chunk in a PNG, payload version 2 base64 in an SVG.

  Checked with Excalidraw's own decoding steps in Node (pako,
  png-chunk-text), non-ASCII text included.
- **Copy image** goes through arboard, because iced's clipboard is text
  only. On GNOME Wayland that is X11 through XWayland, which Mutter hands to
  Wayland apps: checked with `wl-paste` (`cargo test -- --ignored
  clipboard`).
- `--export OUT.png|OUT.svg DRAWING` exports without a window (`SCALE=`,
  `TRANSPARENT=1`, `EMBED=1`, `--theme`).

**Library** (`app/src/library.rs`), as `src/lib/library.ts`:

- **Storage:** core's `library.excalidrawlib`, written as the Tauri app
  writes it.
- **First launch:** starts with the ER shapes. A file that cannot be read is
  never written over; the app says so.
- **Sets:** the ER set and the Kanagawa ERD Wave, Dragon and Lotus sets are
  bundled. They are added with `mergeLibraryItems`'s rule (same element ids
  and `versionNonce` = already there; new items first).
- **Import:** `.excalidrawlib`, versions 1 and 2. **Export:** the whole
  library.
- **Panel:** a panel down the right, one at a time with the theme editor.
  Each item is a picture, drawn by the exporter on another thread, with its
  name as a tooltip.
  - A click puts a copy in the middle of the view: new ids, groups and
    seeds, with bindings kept. This shares the code paste uses
    (`Editor::insert`).
  - × removes an item, after asking.
  - Add selection adds the selection with its labels and frame contents.
- Not done: dragging an item onto the canvas, and selecting several items
  at once.

Checked:

- **Tests:**
  - export unit tests: size and markers, every element kind, font subsets,
    the embedded drawing in both formats, a selection with its files, scale
    and colours by pixel, too large, escaping, frame clipping, label masks;
  - the app flow test extended with the export choices, the selection
    rule, the embedded canvas colour, and the library: seed, merge without
    doubles, insert, add selection, remove, version 1 import, a broken
    import, and an unreadable file left alone.
- **By eye:**
  - PNGs of real drawings, all bundled fonts, CJK fallback, arrow labels;
  - the exported SVG in headless Chromium;
  - screenshots of the Export menu and the Library panel.

92 tests in the workspace, one more by hand (the desktop clipboard).

### Phase 7 result, 2026-11-13

`iced/packaging/package.sh` writes both packages into
`iced/target/packages/`:

- `excalidraw-desktop-iced-VERSION-1.x86_64.rpm`, from `rpmbuild`;
- `excalidraw-desktop-iced_VERSION_amd64.deb`, assembled with `tar` and
  `ar`, since this machine has no `dpkg-deb`.

VERSION is `package.json`'s.

**Built in a Debian 12 container** (`rust:1-bookworm`, podman, crates
cached in a volume). Built on Fedora 45, the binary needs glibc 2.44: glibc
2.43 and 2.44 gave `cosh`, `sinh`, `acosf` and others new symbol versions.
Built on Debian 12 it needs **glibc 2.35**, from `hypot`. So one build runs
on Ubuntu 22.04, Debian 12 and Fedora 36 and later, where the Tauri
packages need glibc 2.39. The packages state the floor:

- RPM: rpm finds it from the binary.
- deb: `libc6 (>= …)`, read from the binary with `objdump`.

`--host` builds with this machine's toolchain instead.

**Beside the Tauri app, not replacing it,** until phase 8 decides:

- package `excalidraw-desktop-iced`, binary `/usr/bin/excalidraw-iced`;
- `excalidraw-desktop-iced.desktop` ("Excalidraw Desktop (Iced)");
- its own config directory.

Both apps claim `.excalidraw` files. The MIME definition and the post-install
and post-remove scripts are `packaging/`'s, reused. The definition is
installed as `excalidraw-desktop-iced.xml`, so neither package owns a file
of the other's (dpkg refuses shared files).

**Desktop integration:**

- The window's Wayland app id and X11 class is `excalidraw-desktop-iced`
  (`APP_ID`), matching the `.desktop` file, so the desktop finds its icon.
  X11 also gets a window icon.
- Icons are installed in hicolor at 32, 64, 128, 256 and 512 px, plus
  scalable.
- `Exec=excalidraw-iced %F`: the single-instance hand-over from phase 4
  turns several files into tabs.

**Dependencies:**

- The binary itself needs only libc, libm and libgcc_s.
- It loads these as it runs (checked with `LD_DEBUG`):
  - libxkbcommon: required;
  - libwayland-client and libwayland-egl, or the X11 libraries, for the
    session's display server: recommended;
  - libvulkan or libEGL: recommended. Without either, iced draws in
    software (tiny-skia).

Checked:

- **`desktop-file-validate`** passes.
- **Clean containers:** the `.deb` on Debian 12 and Ubuntu 22.04
  (`--no-install-recommends`), the `.rpm` on Fedora 42 (no weak deps). On
  each:
  - the package installs; `rpm -V` passes on Fedora;
  - the installed binary exports a drawing, byte-identical on all three and
    to this machine's export;
  - `*.excalidraw` lands in the MIME database, and the `.desktop` file in
    `mimeinfo.cache` for `application/vnd.excalidraw+json`;
  - after removal, the MIME type is gone.
- **On this Fedora 45 Wayland desktop:** the container-built binary opens
  its window and draws with Vulkan.
- **Not done here:** installing the RPM on this machine (it needs root) and
  checking that GNOME shows the icon. Per decision 2, nothing is published.

### Phase 8 result, 2026-11-13: parity review

**Verdict: not at parity.** Everything the README lists as what this app
*adds* to Excalidraw is there, apart from a few shortcuts. The gap is the
editor itself: in the Tauri app that is Excalidraw 0.18.1, and the Iced
build has rebuilt only its core. Most of all, there is **no properties
panel**: a drawn element's colour, fill, stroke, font or opacity cannot be
changed. Decision 2 says the Iced build does not ship until the feature work
is done, so it cannot replace the Tauri app yet.

#### Benchmarks, re-run today

Today's release build against Excalidraw 0.18.1 in WebKitGTK, with the
phase 1 method (`iced/bench/run.sh 10`, `iced/bench/memory.sh`), on the same
machine:

| Scene | Metric | Iced (phase 1) | Iced (today) | Excalidraw (WebKitGTK) | Today vs Excalidraw |
|---|---|---|---|---|---|
| 10k | pan, mean / p95 | 8.1 / 9.6 ms (123 fps) | 8.6 / 11.1 ms (116 fps) | 35.7 / 43 ms (28 fps) | **4.1×** |
| 10k | zoom, mean / p95 | 11.9 / 13.5 ms (84 fps) | 14.6 / 26.4 ms (69 fps) | 26.4 / 80 ms (38 fps) | **1.8×** |
| 10k | CPU per frame, all processes | 5.6 ms | 6.5 ms | 34.6 ms | 5.3× |
| 2k | pan / zoom | 179 / 173 fps | 179 / 138 fps | 59 / 43 fps | |
| 2k | CPU per frame | 1.3 ms | 2.4 ms | 19.8 ms | 8× |
| small | CPU per frame | 0.4 ms | 0.5 ms | 3.2 ms | 6× |
| 10k | memory (PSS, all processes) | 170 MB | 201 MB | 780 MB (Tauri app) | **26%** |
| 2k | memory | 63 MB | 78 MB | 489 MB (Tauri app) | 16% |
| small | memory | 45 MB | 45 MB | 266 MB (Tauri app) | 17% |
| 10k / 2k / small | start to drawing | 235 / 90 / 59 ms | 288 / 91 / 52 ms | 526 / 415 / 505 ms¹ | **2–10×** |
| | package (rpm) | | 35.8 MB | 17.7 MB, plus WebKitGTK and GTK 3 | |
| | oldest glibc | | 2.35 (Debian 12, Ubuntu 22.04) | 2.39 (Fedora 44) | |

¹ As in phase 1: WebView creation to "scene loaded", without the Tauri shell.

Since phase 1, zooming at 10k has dropped from 84 to 69 fps. Phase 3
measured the same drop, and the likely cause is text re-rasterised from the
merged 42 000-glyph Excalifont. Memory at 10k grew by 31 MB, from the merged
fonts and the editor's state. Measured against the gate, memory (at most
half) and start time (faster) still pass, and so does panning at 10k. Zooming
at 10k no longer does: it is 1.8× better, and the gate asked for 2×. That is
work for the renderer (W1), not a reason to stop. The
package is twice the Tauri one, mostly Excalifont with Xiaolai, but it
needs no web engine.

#### Checklist: the README's "What this adds"

| README item | Iced build |
|---|---|
| Tabs, own view per tab, unsaved dot | ✅ Undo history is kept per tab; the Tauri app clears it on a switch |
| `Ctrl+Tab`, `Ctrl+1`…`Ctrl+9` | ✅ |
| `Ctrl+T`, `Ctrl+PageDown`/`PageUp` | ❌ Only `Ctrl+N` and `Ctrl+Tab` |
| Real files, desktop dialogs, atomic writes | ✅ |
| Open Recent (ten) | ✅ Same file and rules |
| Double-click in the file manager, several files into tabs, CLI args | ✅ Tested in containers; not yet installed on this desktop |
| Runs once, a second launch hands over its files | ✅ |
| Snapshots of every tab, recovery after a crash, reopen after a quit | ✅ |
| Ten themes, theme editor, user themes as JSON, follow system | ✅ The colour chooser is in-app sliders instead of the desktop's |
| ER shapes from the first launch; Kanagawa ERD Wave/Dragon/Lotus | ✅ |
| Shortcuts follow the keyboard layout (AZERTY) | ✅ Logical keys |
| …and fall back to the key's position on Cyrillic or Greek | ❌ Shortcuts read only the logical key |
| Native menu bar | ➖ In-app menus (File, Export, Theme), as W4 planned |
| Bundled fonts, no network, no account | ✅ |
| Links open in the system browser | ❌ Links are kept in the file and in SVG, but cannot be followed or edited |
| Embedded web content as a placeholder | ✅ |
| Export: PNG, selection PNG, SVG, clipboard; the three choices | ✅ Compatible with Excalidraw's embedded format |
| Library saved between launches, import/export | ✅ Neither dragging an item onto the canvas nor selecting several works yet |
| Window size and position remembered | ❌ Always opens at the default size |

The Tauri app's known limitations that do not exist here: the grid colour
and the colour-picker palette can no longer be wrong, because there is no
grid and no palette yet. The window title is set by iced, but whether it
updates on GNOME Wayland has not been checked.

#### Checklist: the editor (Excalidraw 0.18.1)

| Area | Iced build has | Missing |
|---|---|---|
| Tools | Hand, select, rectangle, diamond, ellipse, arrow, line, draw, text, tool lock | Image, eraser, frame, laser, lasso, elbow arrows. Mermaid is dropped (decision 1) and embedded web pages are out |
| Element properties | Defaults from the theme | **The whole properties panel:** stroke and background colour, fill style, stroke width and style, sloppiness, edges, arrowheads and arrow type, font family, size and alignment, opacity, copy and paste styles |
| Editing | Move, resize, rotate, multi-select, groups, z-order (keys only), duplicate, delete, nudge, undo and redo, line point editing, text in shapes, arrow labels and bindings | Align and distribute, flip, lock elements, editing links, context menu |
| Clipboard | Excalidraw elements in and out; image out | Pasting plain text as a text element, pasting images |
| Canvas | Pan, wheel and Ctrl+wheel, Ctrl+±, Shift+1 to fit | Zoom buttons and readout, grid and snapping, zen and view modes, search, help, command palette |
| Toolbar | Text labels | Icons |
| Rendering | rough.js and perfect-freehand bit for bit, every font, CJK | Thin lines heavier than Excalidraw's (W1: MSAA) |

#### Not yet checked by hand

- IME typing (fcitx5, ibus), dead keys, and the caret on centred labels
  (phase 3).
- Switching the desktop between light and dark while the app runs
  (phase 5).
- The package installed on this desktop: the icon in GNOME, double-clicking
  a drawing in Files (phase 7).

From the security section, not done: fuzzing the parser, and `cargo-deny`.
The size limits, `forbid(unsafe_code)` and `cargo audit` are in place.

#### Recommendation: keep both, and continue

The performance case from the gate still holds (4× smoother panning, a
quarter to a sixth of the memory, faster starts, an older glibc floor, no
WebKitGTK). It is still worth finishing. It cannot replace the Tauri app
yet, because an editor that cannot recolour a shape is not a drawing app.
Stopping would leave the Tauri app as it is; the `core` extraction already
helps it either way.

To reach parity, three more phases, then this review again:

| # | Phase | Size | Contents |
|---|---|---|---|
| 9 | Properties panel | L | Excalidraw's left panel for the selection and for new elements: colours with its palette and a hex field, fill, stroke, sloppiness, edges, arrowheads, arrow type, font, size, alignment, opacity, layer buttons; copy and paste styles; toolbar icons |
| 10 | The rest of the editor | L | Image tool and pasting images; eraser; links (edit, and open http, https and mailto in the browser); frames; elbow arrows; context menu; align, distribute, flip, lock; zoom controls; grid and snapping; pasting text |
| 11 | Desktop parity | M | Shortcuts by physical key, `Ctrl+T` and `Ctrl+PageUp`/`PageDown`; remembered window size and position; dragging library items; the by-hand checks above; fuzzing and `cargo-deny` |

**Decision, 2026-11-13 (Rafa): keep both and go with phases 9–11.**

### Phase 9 result, 2026-11-13: the properties panel

**Editor** (`editor/src/props.rs`, no UI), following Excalidraw 0.18.1:

- **Which sections show** for the tool and the selection: the predicates
  of `SelectedShapeActions` (`hasBackground`, `hasStrokeWidth`,
  `hasStrokeStyle`, `canChangeRoundness`, arrows, text, labels in shapes).
- **The value each shows** is `getFormValue`'s: the text being typed, else
  what the selection agrees on (nothing picked when it disagrees; the
  current colour for colours), else what new elements get.
- **What a change does** is `actionProperties`'s:
  - It applies to the selection, with labels for colour, opacity and text
    properties, and becomes the `currentItem*` value for new elements.
  - A new sloppiness also gets a new seed.
  - Edges round adaptively (rectangles) or proportionally (the rest).
  - Arrow type (sharp or curved) and the twelve arrowheads, crow's feet
    included. New arrows take the arrow type.
  - Font family (with its line height), size, alignment and vertical
    alignment lay the text out again (`redrawTextBoundingBox`): a label in
    its shape, which grows if it must, and its arrows follow; free text
    keeps its place, centred on a size change
    (`offsetElementAfterFontResize`).
  - Each change is one undo step; while text is typed, it joins the typing.
- Ctrl+Shift+< and > (a tenth smaller or larger), Ctrl+Alt+C and V (copy
  and paste styles, `actionStyles`'s rules for corners, text and
  arrowheads).

**App** (`app/src/properties.rs`, `app/src/icons.rs`):

- A panel on the left, over the drawing, scrolling when taller than the
  window. Colours: the five quick picks, and a popup with Excalidraw's
  palette (fifteen colours at the shade in use, the five shades of the
  current one) and a hex field. Then fill, stroke width, stroke style,
  sloppiness, edges, arrow type, arrowheads (a popup each end), font
  (three quick picks shown in their own face, and a list of all of them),
  size S/M/L/XL, alignment, vertical alignment, opacity, the layer buttons,
  and duplicate, delete, group, ungroup.
- **Icons** for the panel and the tool bar, drawn with iced's canvas
  (iced's SVG support would add a second, older resvg), with the number key
  in the corner and tooltips with the shortcuts, as Excalidraw's.
- Escape closes a popup first; a click on the drawing closes it.
- `--script` takes `prop NAME VALUE` and `popup NAME`, for screenshots.

Checked:

- Tests: five editor tests (what shows for each tool and a mixed
  selection, undo per change, new elements taking the choices, arrow type
  and arrowheads, font changes on free text and labels, styles copied and
  pasted) and an app test through `App::update` (the palette's hex field,
  Escape, layers, duplicate, delete, Ctrl+Alt+C/V).
- By eye: screenshots of the panel for shapes, arrows (arrowhead popup),
  text (palette open) in Light and Kanagawa Wave.

98 tests in the workspace, one more by hand.
