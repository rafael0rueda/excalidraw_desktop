/**
 * Which elements an export of "the selection" covers. On its own, away from
 * `exports.ts`, so that `scripts/check.mjs` can load it without Excalidraw.
 */

/** As much of an Excalidraw element as the selection rules look at. */
export interface SelectableElement {
  id: string;
  frameId?: string | null;
  containerId?: string | null;
}

/**
 * Expands a raw `selectedElementIds` filter with what Excalidraw's own
 * selection deliberately leaves out of it: a bound text label (selected via
 * its container, `containerId`, never by its own id) and the children of a
 * selected frame (`frameId`). Without this, exporting "selection" after
 * select-all silently drops labels and everything inside a selected frame.
 */
export function expandSelection(
  all: readonly SelectableElement[],
  selectedIds: Readonly<Record<string, boolean>>,
): Set<string> {
  const included = new Set(all.filter((el) => selectedIds[el.id]).map((el) => el.id));
  for (const el of all) {
    if (el.frameId && included.has(el.frameId)) included.add(el.id);
  }
  // A second pass: a label bound to a container that a selected frame just
  // pulled in above is not itself selected or framed, only bound.
  for (const el of all) {
    if ("containerId" in el && el.containerId && included.has(el.containerId)) included.add(el.id);
  }
  return included;
}
