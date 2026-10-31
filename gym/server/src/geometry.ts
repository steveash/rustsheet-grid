import type { Rect } from "./image.ts";

/** Range geometry in points, as the bridge's `geometry` op reports it (sheet-relative). */
export interface PointGeometry {
  left: number;
  top: number;
  width: number;
  height: number;
  columns: Array<{ left: number; width: number }>;
  rows: Array<{ top: number; height: number }>;
}

/**
 * Maps sheet points to screen pixels. Found by calibration: a block of cells starting at A1 is
 * painted a marker color, its on-screen box is measured, and the ratio of its pixel size to its
 * point size gives the scale. Valid while the view shows A1 at its top-left and zoom is unchanged.
 */
export interface ScreenMap {
  /** Screen pixel of the sheet's top-left corner (A1's top-left). */
  originX: number;
  originY: number;
  /** Pixels per point on each axis. */
  scaleX: number;
  scaleY: number;
}

/** Builds a ScreenMap from the marker's measured box and the marked range's point geometry. */
export function screenMapFrom(markerBox: Rect, marked: { left: number; top: number; width: number; height: number }): ScreenMap {
  if (marked.width <= 0 || marked.height <= 0) throw new Error("marked range has no area");
  const scaleX = markerBox.width / marked.width;
  const scaleY = markerBox.height / marked.height;
  return {
    originX: markerBox.x - marked.left * scaleX,
    originY: markerBox.y - marked.top * scaleY,
    scaleX,
    scaleY,
  };
}

/** Screen rectangle of a sheet-point rectangle, rounded outward to whole pixels. */
export function toScreen(map: ScreenMap, r: { left: number; top: number; width: number; height: number }): Rect {
  const x0 = Math.floor(map.originX + r.left * map.scaleX);
  const y0 = Math.floor(map.originY + r.top * map.scaleY);
  const x1 = Math.ceil(map.originX + (r.left + r.width) * map.scaleX);
  const y1 = Math.ceil(map.originY + (r.top + r.height) * map.scaleY);
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** Screen rectangle of every cell in a range, row-major. */
export function cellRects(map: ScreenMap, g: PointGeometry): Rect[][] {
  return g.rows.map((row) => g.columns.map((col) => toScreen(map, { left: col.left, top: row.top, width: col.width, height: row.height })));
}

/** Center of a screen rectangle. */
export function center(r: Rect): { x: number; y: number } {
  return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
}

/** A rectangle grown by `px` on every side and clipped to the viewport. */
export function pad(r: Rect, px: number, viewport: { width: number; height: number }): Rect {
  const x = Math.max(0, r.x - px);
  const y = Math.max(0, r.y - px);
  return {
    x,
    y,
    width: Math.min(viewport.width, r.x + r.width + px) - x,
    height: Math.min(viewport.height, r.y + r.height + px) - y,
  };
}
