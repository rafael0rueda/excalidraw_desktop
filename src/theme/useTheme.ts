import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { message } from "@tauri-apps/plugin-dialog";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import {
  listUserThemes,
  loadSettings,
  saveSettings,
  systemColorScheme,
  themesDirPath,
  type Settings,
} from "../lib/api";
import { applyTheme } from "./apply";
import { FALLBACK_THEME_ID, PRESET_THEMES } from "./presets";
import { SYSTEM_THEME, parseTheme, type Theme } from "./types";

/** Mirrors `Settings::default()` on the Rust side. */
const DEFAULT_SETTINGS: Settings = {
  theme: SYSTEM_THEME,
  light_theme: "kanagawa-lotus",
  dark_theme: "kanagawa-wave",
};

export interface ThemeController {
  /** Presets plus user themes, in menu order. */
  themes: Theme[];
  /** The theme actually on screen. */
  active: Theme;
  /** What the user chose: a theme id, or `SYSTEM_THEME`. */
  selection: string;
  /** The two themes `SYSTEM_THEME` switches between. */
  systemPair: { light: Theme | null; dark: Theme | null };
  select: (id: string) => void;
  /** Re-reads the user themes directory. */
  reload: () => Promise<void>;
  /** Repaints the current theme, e.g. after a file supplied its own background. */
  reapply: () => void;
}

/** A user theme with the same id as a preset replaces it, keeping its position. */
function merge(user: Theme[]): Theme[] {
  const byId = new Map(PRESET_THEMES.map((t) => [t.id, t]));
  for (const theme of user) byId.set(theme.id, theme);
  return [...byId.values()];
}

async function readUserThemes(): Promise<{ themes: Theme[]; errors: string[] }> {
  const raw = await listUserThemes().catch(() => [] as unknown[]);
  const themes: Theme[] = [];
  const errors: string[] = [];
  for (const value of raw) {
    const parsed = parseTheme(value);
    if ("theme" in parsed) themes.push(parsed.theme);
    else errors.push(parsed.error);
  }
  return { themes, errors };
}

export function useTheme(api: ExcalidrawImperativeAPI | null): ThemeController {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [userThemes, setUserThemes] = useState<Theme[]>([]);
  const [systemDark, setSystemDark] = useState(false);

  const themes = useMemo(() => merge(userThemes), [userThemes]);

  const pick = useCallback(
    (id: string) =>
      themes.find((t) => t.id === id) ??
      themes.find((t) => t.id === FALLBACK_THEME_ID) ??
      themes[0],
    [themes],
  );

  const active = useMemo(() => {
    if (settings.theme !== SYSTEM_THEME) return pick(settings.theme);
    return pick(systemDark ? settings.dark_theme : settings.light_theme);
  }, [pick, settings, systemDark]);

  // Load persisted state once. Guarded by a ref rather than a cleanup flag
  // because StrictMode double-invokes effects in development.
  const loaded = useRef(false);
  useEffect(() => {
    if (loaded.current) return;
    void (async () => {
      const [stored, scheme, user] = await Promise.all([
        loadSettings().catch(() => DEFAULT_SETTINGS),
        systemColorScheme().catch(() => "light" as const),
        readUserThemes(),
      ]);
      setSettings(stored);
      setSystemDark(scheme === "dark");
      setUserThemes(user.themes);
      // Errors are reported on an explicit reload, not on startup — a broken
      // file in the themes directory should not greet the user with a dialog.
      loaded.current = true;
    })();
  }, []);

  // Persist on change, but not the value we just read back from disk.
  useEffect(() => {
    if (!loaded.current) return;
    saveSettings(settings).catch(() => {});
  }, [settings]);

  const previous = useRef<Theme | null>(null);
  useEffect(() => {
    if (!api) return;
    applyTheme(active, api, previous.current);
    previous.current = active;
  }, [api, active]);

  // GNOME can flip between light and dark while we are running. Re-checking on
  // focus is enough for a desktop app and costs nothing while idle.
  useEffect(() => {
    if (settings.theme !== SYSTEM_THEME) return;
    const refresh = () => {
      systemColorScheme()
        .then((scheme) => setSystemDark(scheme === "dark"))
        .catch(() => {});
    };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [settings.theme]);

  const select = useCallback((id: string) => {
    setSettings((prev) => (prev.theme === id ? prev : { ...prev, theme: id }));
  }, []);

  const reload = useCallback(async () => {
    const { themes: found, errors } = await readUserThemes();
    setUserThemes(found);
    const dir = await themesDirPath().catch(() => "the themes directory");
    const summary = found.length === 1 ? "1 user theme" : `${found.length} user themes`;
    await message(
      errors.length
        ? `Loaded ${summary} from ${dir}.\n\nSkipped:\n${errors.join("\n")}`
        : `Loaded ${summary} from ${dir}.`,
      { title: "Themes reloaded", kind: errors.length ? "warning" : "info" },
    );
  }, []);

  const reapply = useCallback(() => {
    // Passing the active theme as `previous` keeps any colour the user picked
    // by hand while still restoring the themed canvas background.
    applyTheme(active, api, active);
  }, [active, api]);

  const systemPair = useMemo(
    () => ({
      light: themes.find((t) => t.id === settings.light_theme) ?? null,
      dark: themes.find((t) => t.id === settings.dark_theme) ?? null,
    }),
    [themes, settings.light_theme, settings.dark_theme],
  );

  return { themes, active, selection: settings.theme, systemPair, select, reload, reapply };
}
