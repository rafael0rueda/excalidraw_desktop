/** Small hex-colour helpers. Enough to derive hover and border shades. */

type RGB = [number, number, number];

function parse(hex: string): RGB | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  let body = m[1];
  if (body.length === 3) body = body.split("").map((c) => c + c).join("");
  return [
    parseInt(body.slice(0, 2), 16),
    parseInt(body.slice(2, 4), 16),
    parseInt(body.slice(4, 6), 16),
  ];
}

function format([r, g, b]: RGB): string {
  return "#" + [r, g, b].map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("");
}

/** Blends `a` toward `b`. Non-hex inputs (e.g. "transparent") pass through. */
export function mix(a: string, b: string, t: number): string {
  const x = parse(a);
  const y = parse(b);
  if (!x || !y) return a;
  return format([0, 1, 2].map((i) => x[i] + (y[i] - x[i]) * t) as RGB);
}

/**
 * Moves a colour away from its own background: lighter on dark themes, darker
 * on light ones. Hover states read as "more", not "darker", either way.
 */
export function emphasis(hex: string, dark: boolean, t = 0.12): string {
  return mix(hex, dark ? "#ffffff" : "#000000", t);
}

export function alpha(hex: string, a: number): string {
  const rgb = parse(hex);
  if (!rgb) return hex;
  return `rgba(${rgb[0]}, ${rgb[1]}, ${rgb[2]}, ${a})`;
}

/** Perceived lightness, for deciding what to put on top of a colour. */
export function isDark(hex: string): boolean {
  const rgb = parse(hex);
  if (!rgb) return false;
  const [r, g, b] = rgb.map((v) => v / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b < 0.5;
}
