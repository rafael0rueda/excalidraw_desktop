/**
 * The key a Ctrl shortcut is matched against.
 *
 * `key` — what the layout types — is right for letters: on AZERTY the key
 * labelled Z reports `KeyW` as its code, and matching codes would turn Ctrl+Z
 * into Close Tab. But a layout that types no Latin letters (Cyrillic, Greek…)
 * leaves `key` nothing to match, and there the physical key is the only
 * sensible answer. Digits go by position in every layout — AZERTY types `&`
 * for Ctrl+1 — which is also how browsers pick tabs.
 *
 * Deliberately free of imports, so `scripts/check.mjs` can load it.
 */
export function shortcutKey(key: string, code: string): string {
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (/^[\x20-\x7e]$/.test(key)) return key.toLowerCase();
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1].toLowerCase();
  if (code === "Comma") return ",";
  // Named keys — "Tab", "PageDown" — are the same in every layout.
  return key;
}
