import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { confirm, message } from "@tauri-apps/plugin-dialog";
import { isHex } from "../theme/color";
import { TRANSPARENT, invalidKeys, isColorValue, paintable, slugify, uniqueId } from "../theme/draft";
import { SYSTEM_THEME, type Theme, type ThemeColors } from "../theme/types";
import type { ThemeController } from "../theme/useTheme";
import "./themeEditor.css";

const FIELDS: { key: keyof ThemeColors; label: string; hint: string }[] = [
  { key: "canvas", label: "Canvas", hint: "Behind the drawing" },
  { key: "surface", label: "Surface", hint: "Toolbars and panels" },
  { key: "surfaceAlt", label: "Surface alt", hint: "Hovers and borders" },
  { key: "text", label: "Text", hint: "Labels and icons" },
  { key: "textMuted", label: "Text muted", hint: "Secondary labels" },
  { key: "accent", label: "Accent", hint: "Selection and active tool" },
  { key: "accentText", label: "On accent", hint: "Drawn on top of accent" },
  { key: "danger", label: "Danger", hint: "Destructive actions" },
  { key: "stroke", label: "Stroke", hint: "New elements draw in this" },
  { key: "fill", label: "Fill", hint: "New elements fill with this" },
];

function sameTheme(a: Theme, b: Theme): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export interface ThemeEditorProps {
  theme: ThemeController;
  onClose: () => void;
}

export default function ThemeEditor({ theme, onClose }: ThemeEditorProps) {
  // The theme the draft was loaded from, for the modified check and for
  // repairing half-typed colours while painting.
  const [source, setSource] = useState<Theme>(theme.active);
  const [draft, setDraft] = useState<Theme>(theme.active);
  const modified = !sameTheme(draft, source);

  // Paint the draft on every change; drop the preview when the panel goes away
  // so the chosen theme comes back if nothing was saved.
  const preview = theme.preview;
  useEffect(() => {
    preview(paintable(draft, source));
  }, [draft, source, preview]);
  useEffect(() => () => preview(null), [preview]);

  // Changing what the app *uses* reloads the editor onto it, so the panel and
  // the screen never disagree. Saving deliberately does not: you stay on the
  // theme you were editing. The flag distinguishes the two.
  const adopt = useRef(false);
  useEffect(() => {
    if (!adopt.current) return;
    adopt.current = false;
    setSource(theme.chosen);
    setDraft(theme.chosen);
  }, [theme.chosen]);

  const confirmDiscard = useCallback(async () => {
    if (!modified) return true;
    return confirm(`"${draft.name}" has unsaved changes. Discard them?`, {
      title: "Unsaved theme",
      kind: "warning",
      okLabel: "Discard",
      cancelLabel: "Keep editing",
    });
  }, [modified, draft.name]);

  const close = useCallback(async () => {
    if (await confirmDiscard()) onClose();
  }, [confirmDiscard, onClose]);

  const loadTarget = useCallback(
    async (id: string) => {
      const found = theme.themes.find((t) => t.id === id);
      if (!found || !(await confirmDiscard())) return;
      setSource(found);
      setDraft(found);
    },
    [theme.themes, confirmDiscard],
  );

  /** Runs an Appearance change, then reloads the editor onto the result. */
  const changeAppearance = useCallback(
    async (apply: () => void) => {
      if (!(await confirmDiscard())) return;
      adopt.current = true;
      apply();
    },
    [confirmDiscard],
  );

  const setColor = (key: keyof ThemeColors, value: string) =>
    setDraft((prev) => ({ ...prev, colors: { ...prev.colors, [key]: value } }));

  /** Saving also puts the theme on screen for good, in whichever slot applies. */
  const commit = useCallback(
    async (next: Theme) => {
      const invalid = invalidKeys(next);
      if (invalid.length) {
        const named = invalid.map((key) => FIELDS.find((f) => f.key === key)?.label ?? key);
        await message(`Not a colour: ${named.join(", ")}.\n\nUse a hex value like #1f1f28.`, {
          title: "Cannot save theme",
          kind: "error",
        });
        return;
      }
      try {
        await theme.saveTheme(next);
      } catch (err) {
        await message(String(err), { title: "Could not save theme", kind: "error" });
        return;
      }
      if (theme.selection === SYSTEM_THEME) {
        const { light, dark } = theme.systemPair;
        // Editing a dark theme while following the desktop means you want it as
        // your dark theme; same the other way round.
        theme.setSystemPair(
          next.dark ? (light?.id ?? next.id) : next.id,
          next.dark ? next.id : (dark?.id ?? next.id),
        );
      } else {
        theme.select(next.id);
      }
      setSource(next);
      setDraft(next);
    },
    [theme],
  );

  const saveAsNew = useCallback(() => {
    const taken = new Set(theme.themes.map((t) => t.id));
    taken.delete(draft.id);
    void commit({ ...draft, id: uniqueId(slugify(draft.name), taken) });
  }, [commit, draft, theme.themes]);

  const remove = useCallback(async () => {
    const ok = await confirm(`Delete "${draft.name}"?`, {
      title: "Delete theme",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Keep",
    });
    if (!ok) return;
    try {
      await theme.deleteTheme(draft.id);
    } catch (err) {
      await message(String(err), { title: "Could not delete theme", kind: "error" });
      return;
    }
    onClose();
  }, [draft.id, draft.name, theme, onClose]);

  // Excalidraw binds single-key tool shortcuts on the document, so keystrokes
  // typed in here must not travel any further.
  const panel = useRef<HTMLDivElement>(null);
  const onKeyDown = (event: React.KeyboardEvent) => {
    event.stopPropagation();
    if (event.key === "Escape") void close();
  };
  useEffect(() => {
    panel.current?.querySelector<HTMLElement>("select")?.focus();
  }, []);

  const isUserTheme = theme.userThemeIds.has(draft.id);
  const editable = paintable(draft, source);
  const style = {
    "--ed-surface": editable.colors.surface,
    "--ed-input": editable.colors.canvas,
    "--ed-hover": editable.colors.surfaceAlt,
    "--ed-border": editable.colors.surfaceAlt,
    "--ed-text": editable.colors.text,
    "--ed-muted": editable.colors.textMuted,
    "--ed-accent": editable.colors.accent,
    "--ed-accent-text": editable.colors.accentText,
    "--ed-danger": editable.colors.danger,
  } as CSSProperties;

  return (
    <div className="theme-editor" ref={panel} style={style} onKeyDown={onKeyDown}>
      <header>
        <h2>Themes</h2>
        <button className="close" onClick={() => void close()} title="Close" aria-label="Close">
          ×
        </button>
      </header>

      <div className="body">
        <section>
          <h3>Appearance</h3>
          <label className="field">
            <span>Use</span>
            <select
              value={theme.selection}
              onChange={(e) => {
                const id = e.target.value;
                void changeAppearance(() => theme.select(id));
              }}
            >
              <option value={SYSTEM_THEME}>Follow system</option>
              {theme.themes.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          </label>

          {theme.selection === SYSTEM_THEME && (
            <>
              <label className="field">
                <span>When the desktop is light</span>
                <select
                  value={theme.systemPair.light?.id ?? ""}
                  onChange={(e) => {
                    const id = e.target.value;
                    void changeAppearance(() =>
                      theme.setSystemPair(id, theme.systemPair.dark?.id ?? id),
                    );
                  }}
                >
                  {theme.themes.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>When the desktop is dark</span>
                <select
                  value={theme.systemPair.dark?.id ?? ""}
                  onChange={(e) => {
                    const id = e.target.value;
                    void changeAppearance(() =>
                      theme.setSystemPair(theme.systemPair.light?.id ?? id, id),
                    );
                  }}
                >
                  {theme.themes.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name}
                    </option>
                  ))}
                </select>
              </label>
            </>
          )}
        </section>

        <section>
          <h3>Edit{modified ? " — unsaved changes" : ""}</h3>
          <label className="field">
            <span>Start from</span>
            <select value={source.id} onChange={(e) => void loadTarget(e.target.value)}>
              {theme.themes.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                  {theme.userThemeIds.has(t.id) ? " (custom)" : ""}
                </option>
              ))}
            </select>
          </label>

          <label className="field">
            <span>Name</span>
            <input
              type="text"
              value={draft.name}
              onChange={(e) => setDraft((prev) => ({ ...prev, name: e.target.value }))}
            />
          </label>

          <label className="toggle">
            <input
              type="checkbox"
              checked={draft.dark}
              onChange={(e) => setDraft((prev) => ({ ...prev, dark: e.target.checked }))}
            />
            <span>Dark colour scheme</span>
          </label>
          <p className="hint">
            Affects which way hover shades move and which slot the theme fills when following the
            desktop. The canvas is never inverted.
          </p>
        </section>

        <section>
          <h3>Colours</h3>
          <div className="colors">
            {FIELDS.map(({ key, label, hint }) => {
              const value = draft.colors[key];
              const transparent = value === TRANSPARENT;
              return (
                <div className="color-row" key={key}>
                  <span className="swatch">
                    <input
                      type="color"
                      value={isHex(value) ? value : "#ffffff"}
                      // A transparent fill has no colour to show; the checker
                      // board behind it is the honest answer.
                      style={{ opacity: transparent ? 0 : 1 }}
                      onChange={(e) => setColor(key, e.target.value)}
                      aria-label={label}
                    />
                  </span>
                  <span className="name">
                    {label}
                    <small>{hint}</small>
                  </span>
                  <input
                    type="text"
                    className={`hex${isColorValue(value) ? "" : " invalid"}`}
                    value={value}
                    spellCheck={false}
                    onChange={(e) => setColor(key, e.target.value.trim())}
                  />
                </div>
              );
            })}
          </div>
          <p className="hint">
            <code>transparent</code> is accepted for Fill.
          </p>
        </section>

        <section>
          <h3>Files</h3>
          <p className="hint">
            Save writes <code>{draft.id}.json</code>
            {isUserTheme ? "" : ", which from then on replaces the built-in theme of that id"}. Save
            as new derives a fresh id from the name and leaves the original alone.
          </p>
          <p className="hint">
            Themes are plain JSON in <code>{theme.themesDir || "the themes directory"}</code>.
            Editing one by hand and choosing Reload user themes works just as well.
          </p>
        </section>
      </div>

      <footer>
        <button className="primary" onClick={() => void commit(draft)}>
          Save
        </button>
        <button onClick={saveAsNew} disabled={!draft.name.trim()}>
          Save as new
        </button>
        {isUserTheme && (
          <button className="danger" onClick={() => void remove()}>
            Delete
          </button>
        )}
      </footer>
    </div>
  );
}
