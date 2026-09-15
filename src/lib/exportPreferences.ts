import { useCallback, useEffect, useRef, useState } from "react";
import { loadExportPreferences, saveExportPreferences, type ExportPreferences } from "./api";

/** Mirrors `ExportPreferences::default()` on the Rust side. */
const DEFAULTS: ExportPreferences = { scale: 2, transparent: false, embed_scene: false };

/** The Export menu's choices, remembered in ~/.config/excalidraw-desktop/export.json. */
export function useExportPreferences() {
  const [preferences, setPreferences] = useState<ExportPreferences>(DEFAULTS);
  /** What was read from disk, which there is no point writing straight back. */
  const fromDisk = useRef<ExportPreferences | null>(null);
  const saving = useRef(Promise.resolve());

  useEffect(() => {
    loadExportPreferences()
      .then((stored) => {
        // StrictMode runs this effect twice in development; the second read
        // must not take the first one's result for a choice the user made.
        const before = fromDisk.current;
        fromDisk.current = stored;
        // A choice made before this resolved wins, and is copied so it gets saved.
        setPreferences((prev) => (prev === DEFAULTS || prev === before ? stored : { ...prev }));
      })
      .catch(() => {});
  }, []);

  // Queued, so two quick changes cannot reach disk in the wrong order.
  useEffect(() => {
    if (preferences === DEFAULTS || preferences === fromDisk.current) return;
    saving.current = saving.current.then(() => saveExportPreferences(preferences)).catch(() => {});
  }, [preferences]);

  const update = useCallback(
    (change: Partial<ExportPreferences>) => setPreferences((prev) => ({ ...prev, ...change })),
    [],
  );

  return { preferences, update };
}
