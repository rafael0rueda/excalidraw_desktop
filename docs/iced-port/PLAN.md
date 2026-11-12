# Iced port — plan

Branch: `iced-port` (from `main` at 75225c2). Written 2026-11-11.
Status: **plan only, nothing built yet.**

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
| 4–8 | | Not started |

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

