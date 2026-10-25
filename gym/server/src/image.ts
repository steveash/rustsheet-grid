import { createHash } from "node:crypto";
import { PNG } from "pngjs";

/** A rectangle in screen pixels. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/**
 * Bounding box of the pixels exactly equal to `color` (alpha ignored), or undefined when none
 * match. Calibration paints a block of cells in a color Excel never draws, so an exact match is
 * reliable and immune to antialiasing at the block's edges being slightly off.
 */
export function findColorBox(png: Buffer, color: Rgb): Rect | undefined {
  const img = PNG.sync.read(png);
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < img.height; y++) {
    for (let x = 0; x < img.width; x++) {
      const i = (y * img.width + x) * 4;
      if (img.data[i] === color.r && img.data[i + 1] === color.g && img.data[i + 2] === color.b) {
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
      }
    }
  }
  if (maxX < 0) return undefined;
  return { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 };
}

/** Whether every pixel of the image is the same color (a blank or not-yet-painted frame). */
export function isUniform(png: Buffer): boolean {
  const img = PNG.sync.read(png);
  const d = img.data;
  for (let i = 4; i < d.length; i += 4) {
    if (d[i] !== d[0] || d[i + 1] !== d[1] || d[i + 2] !== d[2]) return false;
  }
  return true;
}

export function digest(png: Buffer): string {
  return createHash("sha256").update(png).digest("hex");
}

/**
 * Captures frames until two consecutive ones are identical, so animations (Excel eases the
 * selection outline and fades in recalculated cells) have settled. Returns the last frame and
 * whether it settled before `timeoutMs`.
 */
export async function captureStable(
  capture: () => Promise<Buffer>,
  { intervalMs = 120, timeoutMs = 5_000 } = {},
): Promise<{ png: Buffer; stable: boolean; frames: number }> {
  const deadline = Date.now() + timeoutMs;
  let prev = await capture();
  let frames = 1;
  for (;;) {
    await new Promise((r) => setTimeout(r, intervalMs));
    const next = await capture();
    frames++;
    if (digest(next) === digest(prev)) return { png: next, stable: true, frames };
    prev = next;
    if (Date.now() >= deadline) return { png: prev, stable: false, frames };
  }
}
